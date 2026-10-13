use super::*;
use crate::models::RequestDraft;
#[test]
fn restart_and_stale_save_preserve_committed_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("requests.sqlite3");
    let repository = DocumentRepository::new(path.clone());
    assert!(repository.load().unwrap().documents.is_empty());
    let doc = RequestDocument::snapshot(
        uuid::Uuid::new_v4().to_string(),
        "Echo".into(),
        1,
        &RequestDraft::new(),
        "",
        "",
    );
    repository.save(&doc).unwrap();
    assert!(repository.save(&doc).is_err());
    let repository = DocumentRepository::new(path);
    assert!(repository.load().unwrap().documents == vec![doc]);
}

#[test]
fn failed_write_unknown_versions_and_corrupt_records_are_non_destructive() {
    let dir = tempfile::tempdir().unwrap();
    let repository = DocumentRepository::new(dir.path().join("requests.sqlite3"));
    let doc = RequestDocument::snapshot(
        uuid::Uuid::new_v4().to_string(),
        "Keep".into(),
        1,
        &RequestDraft::new(),
        "",
        "",
    );
    repository.save(&doc).unwrap();
    let connection = Connection::open(repository.path()).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE ON request_documents BEGIN SELECT RAISE(ABORT, 'private-secret-must-not-leak'); END;").unwrap();
    let next = RequestDocument::snapshot(
        doc.id().into(),
        "New".into(),
        2,
        &RequestDraft::new(),
        "",
        "",
    );
    let error = repository.save(&next).unwrap_err();
    assert!(!error.contains("private-secret"));
    assert!(repository.load().unwrap().documents == vec![doc.clone()]);
    connection
        .execute(
            "INSERT INTO request_documents VALUES ('broken', 1, 'private-secret')",
            [],
        )
        .unwrap();
    let mut unknown = serde_json::to_value(&doc).unwrap();
    unknown["version"] = 999.into();
    let id = uuid::Uuid::new_v4().to_string();
    unknown["id"] = id.clone().into();
    connection
        .execute(
            "INSERT INTO request_documents VALUES (?1, 1, ?2)",
            params![id, unknown.to_string()],
        )
        .unwrap();
    let loaded = repository.load().unwrap();
    assert_eq!(loaded.skipped, 2);
    assert!(loaded.documents == vec![doc]);
    connection.execute_batch("PRAGMA user_version=999").unwrap();
    assert!(repository.load().is_err());
    assert!(repository.save(&next).is_err());
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM request_documents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
}
