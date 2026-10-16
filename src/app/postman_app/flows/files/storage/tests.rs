use super::*;
use postman_flow::{compile_flow, CompileEnvironment};

#[test]
fn native_round_trips_preserve_catalog_loops_and_json_values() {
    let dir = tempfile::tempdir().unwrap();
    for (ix, text) in [
        include_str!(
            "../../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
        ),
        include_str!(
            "../../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
        ),
        include_str!(
            "../../../../../../crates/postman-flow/examples/flows/httpbingo_json_values.http.yml"
        ),
    ]
    .iter()
    .enumerate()
    {
        let document = parse_flow_yaml(text).unwrap();
        let path = dir.path().join(format!("{ix}.http.yml"));
        let bytes = encode(&document).unwrap();
        write(&path, &bytes, None).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.document, document);
        assert_eq!(loaded.bytes, bytes);
        assert!(compile_flow(
            &document.flow,
            &document.apis,
            &CompileEnvironment::default()
        )
        .is_ok());
    }
}
#[test]
fn conflicts_failed_replacement_and_readonly_files_preserve_previous_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    fs::write(&path, "old").unwrap();
    assert!(
        matches!(write(&path, b"new", None), Err(WriteError::Conflict(Some(bytes))) if bytes == b"old")
    );
    assert!(matches!(
        write(&path, b"new", Some(b"stale")),
        Err(WriteError::Conflict(_))
    ));
    assert!(replace(&path, b"new", || Err(WriteError::Failed(
        "injected replacement failure".into()
    )))
    .is_err());
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    write(&path, b"new", Some(b"old")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    assert!(matches!(
        write(&path, b"lost", Some(b"new")),
        Err(WriteError::Failed(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"new");
    fs::set_permissions(&path, permissions).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        write(&path, b"new", Some(b"old")),
        Err(WriteError::Conflict(None))
    ));
    assert!(write(&dir.path().join("missing/file.http.yml"), b"new", None).is_err());
}
#[test]
fn bounded_open_and_recent_index_fail_without_rewriting_sources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    for text in [
        b"invalid yaml: [".as_slice(),
        b"version: 999\nflow: {}",
        &[0xff, 0xfe],
    ] {
        fs::write(&path, text).unwrap();
        assert!(open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), text);
    }
    fs::write(&path, vec![b'x'; MAX_DOCUMENT as usize + 1]).unwrap();
    assert!(open(&path).is_err());
    let mut too_large = parse_flow_yaml(include_str!(
        "../../../../../../crates/postman-flow/examples/flows/httpbingo_minimal.http.yml"
    ))
    .unwrap();
    too_large.flow.name = "x".repeat(MAX_DOCUMENT as usize);
    assert!(
        encode(&too_large).is_err(),
        "Save cannot write a file that Open rejects for size"
    );
    assert!(open(dir.path()).is_err());
    let index = dir.path().join("recent.json");
    let mut recent = load_recent(&index).unwrap();
    remember(&mut recent, path.clone(), "Original".into());
    remember(&mut recent, path.clone(), "Updated".into());
    save_recent(&index, &recent).unwrap();
    assert_eq!(load_recent(&index).unwrap(), recent);
    assert_eq!(recent.len(), 1);
    save_recent(&index, &[recent[0].clone(), recent[0].clone()]).unwrap();
    assert!(load_recent(&index).is_err());
    fs::write(&index, b"broken index").unwrap();
    assert!(load_recent(&index).is_err());
    assert_eq!(fs::read(&index).unwrap(), b"broken index");
}

#[test]
fn moving_yaml_preserves_file_expressions_without_reading_body_files() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("another-directory");
    fs::create_dir(&target).unwrap();
    let mut doc = parse_flow_yaml(include_str!(
        "../../../../../../crates/postman-flow/examples/flows/httpbingo_minimal.http.yml"
    ))
    .unwrap();
    let postman_flow::HttpRequestSource::Inline(request) = &mut doc.flow.steps[0].request else {
        panic!()
    };
    request.body = postman_flow::BodyTemplate::File(postman_flow::TextTemplate::literal(
        "body-that-does-not-exist.bin",
    ));
    let bytes = encode(&doc).unwrap();
    for path in [
        dir.path().join("original.http.yml"),
        target.join("moved.http.yml"),
    ] {
        write(&path, &bytes, None).unwrap();
        assert_eq!(open(&path).unwrap().document, doc);
    }
    assert!(!dir.path().join("body-that-does-not-exist.bin").exists());
}
