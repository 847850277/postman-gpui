//! File-backed request documents. Each operation runs on the background executor; writes use
//! optimistic revisions inside SQLite transactions so stale saves cannot overwrite newer data.
use crate::models::request_document::RequestDocument;
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::PathBuf, time::Duration};

#[derive(Clone)]
pub(crate) struct DocumentRepository {
    path: PathBuf,
}
pub(crate) struct DocumentLoad {
    pub documents: Vec<RequestDocument>,
    pub skipped: usize,
}
impl DocumentRepository {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
    fn connect(&self) -> Result<Connection, String> {
        let open = || -> anyhow::Result<Connection> {
            let parent = self
                .path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing directory"))?;
            fs::create_dir_all(parent)?;
            // Documents contain explicitly saved secrets. Restrict a newly created file on Unix.
            if !self.path.exists() {
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                match options.open(&self.path) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
            let mut connection = Connection::open(&self.path)?;
            connection.busy_timeout(Duration::from_secs(2))?;
            let transaction = connection.transaction()?;
            let version: u32 =
                transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
            match version {
                0 => {
                    transaction.execute_batch("CREATE TABLE request_documents (id TEXT PRIMARY KEY, revision INTEGER NOT NULL, snapshot TEXT NOT NULL); PRAGMA user_version = 1;")?;
                }
                1 => {}
                _ => anyhow::bail!("Unsupported database version"),
            }
            transaction.commit()?;
            Ok(connection)
        };
        // Avoid echoing request values or SQLite payloads into diagnostics.
        open().map_err(|_| format!("Cannot open saved requests at {}. Restore access or a compatible database, then reload.", self.path.display()))
    }
    pub(crate) fn load(&self) -> Result<DocumentLoad, String> {
        let connection = self.connect()?;
        let load = || -> rusqlite::Result<DocumentLoad> {
            let mut statement = connection.prepare(
                "SELECT id, revision, snapshot FROM request_documents ORDER BY rowid DESC",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            let mut result = DocumentLoad {
                documents: vec![],
                skipped: 0,
            };
            for row in rows {
                let valid = row.ok().and_then(|(id, revision, json)| {
                    let document: RequestDocument = serde_json::from_str(&json).ok()?;
                    (document.validate().is_ok()
                        && document.id() == id
                        && document.revision() as i64 == revision)
                        .then_some(document)
                });
                if let Some(document) = valid {
                    result.documents.push(document);
                } else {
                    result.skipped += 1;
                }
            }
            Ok(result)
        };
        load().map_err(|_| "Could not read saved requests. Your stored documents have not been changed. Reload to retry.".into())
    }
    pub(crate) fn save(&self, document: &RequestDocument) -> Result<(), String> {
        document.validate()?;
        let snapshot = serde_json::to_string(document).map_err(|_| {
            "Could not encode this request. Check file paths for unsupported characters."
        })?;
        let mut connection = self.connect()?;
        let mut save = || -> rusqlite::Result<bool> {
            let transaction = connection.transaction()?;
            let previous: Option<(i64, String)> = transaction
                .query_row(
                    "SELECT revision, snapshot FROM request_documents WHERE id = ?1",
                    [document.id()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let valid = match previous {
                None => document.revision() == 1,
                Some((revision, json)) => {
                    revision.checked_add(1) == Some(document.revision() as i64)
                        && serde_json::from_str::<RequestDocument>(&json).is_ok_and(|old| {
                            old.validate().is_ok()
                                && old.id() == document.id()
                                && old.revision() as i64 == revision
                        })
                }
            };
            if !valid {
                return Ok(false);
            }
            transaction.execute("INSERT INTO request_documents (id, revision, snapshot) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET revision = excluded.revision, snapshot = excluded.snapshot", params![document.id(), document.revision() as i64, snapshot])?;
            transaction.commit()?;
            Ok(true)
        };
        match save() {
            Ok(true) => Ok(()),
            Ok(false) => Err("This document changed on disk or cannot be decoded. Your draft is intact; reopen the saved version in a new session before retrying.".into()),
            Err(_) => Err("Could not save request. Check available disk space and permissions, then retry. Your draft and last saved version are intact.".into()),
        }
    }
    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests;
