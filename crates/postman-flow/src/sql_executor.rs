use std::time::Instant;
use postman_http::response::HttpResponse;
use serde_json::Value;

pub async fn execute_sql(
    connection_str: &str,
    query_str: &str,
    params: &[String],
) -> Result<HttpResponse, String> {
    let connection_str = connection_str.trim().to_string();
    let query_str = query_str.trim().to_string();
    let params = params.to_vec();

    let start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        if connection_str.starts_with("sqlite:") || connection_str.starts_with("sqlite://") || connection_str == ":memory:" {
            execute_sqlite(&connection_str, &query_str, &params)
        } else if connection_str.starts_with("mysql://") {
            execute_mysql(&connection_str, &query_str, &params)
        } else {
            Err(format!(
                "unsupported database connection scheme in '{}', expected 'sqlite:' or 'mysql://'",
                connection_str
            ))
        }
    })
    .await
    .map_err(|err| format!("SQL execution thread join error: {err}"))?;

    let elapsed_ms = start.elapsed().as_millis();

    match result {
        Ok(json_body) => {
            let body_str = serde_json::to_string(&json_body).unwrap_or_default();
            let mut response = HttpResponse::new(
                200,
                vec![("content-type".to_string(), "application/json".to_string())],
                body_str,
            );
            response.elapsed_ms = elapsed_ms;
            Ok(response)
        }
        Err(err_msg) => {
            let error_json = serde_json::json!({
                "error": err_msg,
                "code": "SQL_ERROR"
            });
            let body_str = serde_json::to_string(&error_json).unwrap_or_default();
            let mut response = HttpResponse::new(
                400,
                vec![("content-type".to_string(), "application/json".to_string())],
                body_str,
            );
            response.elapsed_ms = elapsed_ms;
            Ok(response)
        }
    }
}

fn is_query_returning_rows(query: &str) -> bool {
    let trimmed = query.trim_start();
    let first_word = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_uppercase();
    matches!(
        first_word.as_str(),
        "SELECT" | "PRAGMA" | "EXPLAIN" | "DESCRIBE" | "DESC" | "SHOW" | "WITH"
    )
}

fn execute_sqlite(
    connection_str: &str,
    query_str: &str,
    params: &[String],
) -> Result<Value, String> {
    let conn = if connection_str == ":memory:" || connection_str == "sqlite::memory:" || connection_str == "sqlite://:memory:" {
        rusqlite::Connection::open_in_memory()
    } else {
        let path = if let Some(stripped) = connection_str.strip_prefix("sqlite://") {
            stripped
        } else if let Some(stripped) = connection_str.strip_prefix("sqlite:") {
            stripped
        } else {
            connection_str
        };
        rusqlite::Connection::open(path)
    }
    .map_err(|e| format!("SQLite connection error: {e}"))?;

    if is_query_returning_rows(query_str) {
        let mut stmt = conn
            .prepare(query_str)
            .map_err(|e| format!("SQLite prepare error: {e}"))?;
        let col_names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
        let mut rows = Vec::new();
        let mut query_rows = stmt
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| format!("SQLite query error: {e}"))?;

        while let Some(row) = query_rows.next().map_err(|e| format!("SQLite row error: {e}"))? {
            let mut obj = serde_json::Map::new();
            for (i, col_name) in col_names.iter().enumerate() {
                let val: rusqlite::types::Value = row.get(i).map_err(|e| format!("SQLite get col error: {e}"))?;
                let json_val = match val {
                    rusqlite::types::Value::Null => Value::Null,
                    rusqlite::types::Value::Integer(v) => serde_json::json!(v),
                    rusqlite::types::Value::Real(v) => serde_json::json!(v),
                    rusqlite::types::Value::Text(v) => serde_json::json!(v),
                    rusqlite::types::Value::Blob(v) => serde_json::json!(hex::encode(v)),
                };
                obj.insert(col_name.clone(), json_val);
            }
            rows.push(Value::Object(obj));
        }

        let count = rows.len();
        Ok(serde_json::json!({
            "rows": rows,
            "count": count
        }))
    } else {
        let rows_affected = conn
            .execute(query_str, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| format!("SQLite execute error: {e}"))?;
        let last_insert_id = conn.last_insert_rowid();
        Ok(serde_json::json!({
            "rows_affected": rows_affected,
            "last_insert_id": last_insert_id
        }))
    }
}

fn execute_mysql(
    connection_str: &str,
    query_str: &str,
    params: &[String],
) -> Result<Value, String> {
    use mysql::prelude::*;

    let opts = mysql::Opts::from_url(connection_str)
        .map_err(|e| format!("MySQL connection URL error: {e}"))?;
    let mut conn = mysql::Conn::new(opts).map_err(|e| format!("MySQL connection error: {e}"))?;

    let mysql_params = mysql::Params::Positional(
        params
            .iter()
            .map(|s| mysql::Value::from(s.as_str()))
            .collect(),
    );

    if is_query_returning_rows(query_str) {
        let query_result = conn
            .exec_iter(query_str, mysql_params)
            .map_err(|e| format!("MySQL query error: {e}"))?;
        let columns: Vec<String> = query_result
            .columns()
            .as_ref()
            .iter()
            .map(|c| c.name_str().to_string())
            .collect();
        let mut rows = Vec::new();

        for row in query_result {
            let row = row.map_err(|e| format!("MySQL row error: {e}"))?;
            let mut obj = serde_json::Map::new();
            for (i, col_name) in columns.iter().enumerate() {
                let val: mysql::Value = row.get(i).unwrap_or(mysql::Value::NULL);
                let json_val = match val {
                    mysql::Value::NULL => Value::Null,
                    mysql::Value::Bytes(b) => {
                        if let Ok(s) = String::from_utf8(b.clone()) {
                            serde_json::json!(s)
                        } else {
                            serde_json::json!(hex::encode(b))
                        }
                    }
                    mysql::Value::Int(v) => serde_json::json!(v),
                    mysql::Value::UInt(v) => serde_json::json!(v),
                    mysql::Value::Float(v) => serde_json::json!(v),
                    mysql::Value::Double(v) => serde_json::json!(v),
                    mysql::Value::Date(y, m, d, h, mi, s, u) => {
                        serde_json::json!(format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}.{u:06}"))
                    }
                    mysql::Value::Time(neg, d, h, mi, s, u) => {
                        serde_json::json!(format!(
                            "{}{d}d {h:02}:{mi:02}:{s:02}.{u:06}",
                            if neg { "-" } else { "" }
                        ))
                    }
                };
                obj.insert(col_name.clone(), json_val);
            }
            rows.push(Value::Object(obj));
        }

        let count = rows.len();
        Ok(serde_json::json!({
            "rows": rows,
            "count": count
        }))
    } else {
        conn.exec_drop(query_str, mysql_params)
            .map_err(|e| format!("MySQL execute error: {e}"))?;
        let rows_affected = conn.affected_rows();
        let last_insert_id = conn.last_insert_id();
        Ok(serde_json::json!({
            "rows_affected": rows_affected,
            "last_insert_id": last_insert_id
        }))
    }
}
