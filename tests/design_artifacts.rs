use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const PENCIL_VERSION: &str = "2.17";
const ROOT_WIDTH: i64 = 1600;
const ROOT_PADDING: i64 = 32;
const ROOT_GAP: i64 = 24;
const APP_HEIGHT: i64 = 900;

const SHARED_TOKENS: &[(&str, &str, &str)] = &[
    ("bg", "color", "#F4F7F3"),
    ("panel", "color", "#FFFEFB"),
    ("panelAlt", "color", "#EEF5F1"),
    ("line", "color", "#CFE0D7"),
    ("text", "color", "#20342B"),
    ("muted", "color", "#526B60"),
    ("subtle", "color", "#64786E"),
    ("accent", "color", "#C64B2B"),
    ("accentSoft", "color", "#FFF0E8"),
    ("green", "color", "#0E7A4E"),
    ("greenSoft", "color", "#E4F6EA"),
    ("blue", "color", "#0F718B"),
    ("blueSoft", "color", "#E6F4F7"),
    ("code", "color", "#F0F5F1"),
    ("codeText", "color", "#243D34"),
    ("fontBody", "string", "Inter"),
    ("fontMono", "string", "JetBrains Mono"),
    ("accentVivid", "color", "#F56B3D"),
    ("accentInk", "color", "#3C1F16"),
];

#[test]
fn issue_design_artifacts_follow_the_shared_contract() {
    let design_dir = manifest_dir().join("design");
    let readme = fs::read_to_string(design_dir.join("README.md"))
        .expect("design/README.md should be readable");
    let spec =
        fs::read_to_string(design_dir.join("SPEC.md")).expect("design/SPEC.md should be readable");
    assert!(
        spec.contains("## 4. Content Items"),
        "SPEC.md must define the content-item contract"
    );

    let mut artifacts = fs::read_dir(&design_dir)
        .expect("design directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("issue-") && name.ends_with(".pen"))
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    assert!(
        !artifacts.is_empty(),
        "at least one issue design is required"
    );

    for path in artifacts {
        validate_issue_artifact(&path, &readme);
    }
}

#[test]
fn reusable_design_template_follows_the_shared_shell() {
    let path = manifest_dir()
        .join("design")
        .join("templates")
        .join("e2e-feature.pen");
    let document = read_document(&path);
    validate_document_primitives(&path, &document);

    let root = single_root(&path, &document);
    assert_canonical_root(&path, root);
    assert_section_order(&path, root);
    assert_application_shell(&path, root);

    let all_text = collect_text(&document);
    assert!(
        all_text.contains("Issue #NN") && all_text.contains("issue-00NN-feature.pen"),
        "{} must retain obvious placeholders",
        path.display()
    );
}

fn validate_issue_artifact(path: &Path, readme: &str) {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("design filename should be UTF-8");
    let issue = issue_number(file_name)
        .unwrap_or_else(|| panic!("{file_name} does not match issue-NNNN-feature.pen"));
    let document = read_document(path);

    validate_document_primitives(path, &document);
    let root = single_root(path, &document);
    assert_canonical_root(path, root);
    assert_section_order(path, root);
    assert_application_shell(path, root);
    assert_root_contains_children(path, root);

    let root_name = string_field(root, "name", path);
    assert!(
        root_name.contains(&format!("Issue #{issue}")),
        "{} root name must contain Issue #{}",
        path.display(),
        issue
    );

    let all_text = collect_text(&document);
    assert!(
        all_text.contains(&format!("issues/{issue}")),
        "{} must contain its GitHub issue URL",
        path.display()
    );
    assert!(
        all_text.contains(file_name),
        "{} E2E contract must contain its design filename",
        path.display()
    );
    assert!(
        all_text.contains("tests/"),
        "{} E2E contract must contain a scenario or test path",
        path.display()
    );
    assert!(
        readme.contains(&format!("[#{issue} ")) && readme.contains(file_name),
        "design/README.md must map Issue #{} to {}",
        issue,
        file_name
    );
}

fn validate_document_primitives(path: &Path, document: &Value) {
    assert_eq!(
        document.get("version").and_then(Value::as_str),
        Some(PENCIL_VERSION),
        "{} must use Pencil version {}",
        path.display(),
        PENCIL_VERSION
    );
    assert_shared_tokens(path, document);
    assert_unique_ids(path, document);

    let token = document
        .get("fileToken")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{} must define fileToken", path.display()));
    assert!(
        is_uuid_v4(token),
        "{} fileToken must be a UUID v4, got {token}",
        path.display()
    );
}

fn assert_shared_tokens(path: &Path, document: &Value) {
    let variables = document
        .get("variables")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} must define variables", path.display()));
    assert_eq!(
        variables.len(),
        SHARED_TOKENS.len(),
        "{} must define exactly the shared token set",
        path.display()
    );

    for (name, token_type, value) in SHARED_TOKENS {
        let token = variables
            .get(*name)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{} is missing token {name}", path.display()));
        assert_eq!(
            token.get("type").and_then(Value::as_str),
            Some(*token_type),
            "{} token {name} has the wrong type",
            path.display()
        );
        assert_eq!(
            token.get("value").and_then(Value::as_str),
            Some(*value),
            "{} token {name} has the wrong value",
            path.display()
        );
    }
}

fn assert_unique_ids(path: &Path, document: &Value) {
    let mut seen = HashSet::new();
    let mut count = 0;
    walk(document, &mut |value| {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            return;
        };
        assert!(
            !id.is_empty(),
            "{} contains an empty node id",
            path.display()
        );
        assert!(
            seen.insert(id.to_string()),
            "{} contains duplicate node id {id}",
            path.display()
        );
        count += 1;
    });
    assert!(
        count > 0,
        "{} must contain identified nodes",
        path.display()
    );
}

fn assert_canonical_root(path: &Path, root: &Value) {
    assert_eq!(
        number_field(root, "x", path),
        0,
        "{} root x",
        path.display()
    );
    assert_eq!(
        number_field(root, "y", path),
        0,
        "{} root y",
        path.display()
    );
    assert_eq!(
        number_field(root, "width", path),
        ROOT_WIDTH,
        "{} root width",
        path.display()
    );
    assert_eq!(
        string_field(root, "fill", path),
        "$bg",
        "{} root fill",
        path.display()
    );
    assert_eq!(
        string_field(root, "layout", path),
        "vertical",
        "{} root layout",
        path.display()
    );
    assert_eq!(
        number_field(root, "gap", path),
        ROOT_GAP,
        "{} root gap",
        path.display()
    );
    assert_eq!(
        number_field(root, "padding", path),
        ROOT_PADDING,
        "{} root padding",
        path.display()
    );
    assert_eq!(
        root.get("clip").and_then(Value::as_bool),
        Some(true),
        "{} root must clip",
        path.display()
    );
}

fn assert_section_order(path: &Path, root: &Value) {
    let children = child_nodes(root, path);
    assert!(
        children.len() >= 3,
        "{} needs Design Header, feature stage, and E2E Contract",
        path.display()
    );
    assert_eq!(
        string_field(children[0], "name", path),
        "Design Header",
        "{} first section",
        path.display()
    );
    assert_eq!(
        number_field(children[0], "height", path),
        150,
        "{} Design Header height",
        path.display()
    );

    let contract = children.last().expect("children is not empty");
    assert_eq!(
        string_field(contract, "name", path),
        "E2E Contract",
        "{} final section",
        path.display()
    );
    assert!(
        matches!(number_field(contract, "height", path), 480..=500),
        "{} E2E Contract height must be 480–500px",
        path.display()
    );

    let has_feature_stage = children[1..children.len() - 1].iter().any(|child| {
        child
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| {
                name.starts_with("Application State") || name.starts_with("State Row")
            })
    });
    assert!(
        has_feature_stage,
        "{} must contain an Application State or State Row",
        path.display()
    );
}

fn assert_application_shell(path: &Path, root: &Value) {
    let children = child_nodes(root, path);
    for app in children.iter().filter(|child| {
        child
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.starts_with("Application State"))
    }) {
        assert_eq!(
            number_field(app, "height", path),
            APP_HEIGHT,
            "{} full application height",
            path.display()
        );
        assert_eq!(
            app.get("width").and_then(Value::as_str),
            Some("fill_container"),
            "{} full application width",
            path.display()
        );

        let app_children = child_nodes(app, path);
        let header = named_child(&app_children, "Top Header")
            .unwrap_or_else(|| panic!("{} application must contain Top Header", path.display()));
        assert_eq!(
            number_field(header, "height", path),
            72,
            "{} Top Header height",
            path.display()
        );
        let body = app_children
            .iter()
            .copied()
            .find(|node| string_field(node, "name", path).contains("Body"))
            .unwrap_or_else(|| panic!("{} application must contain a body", path.display()));
        let body_children = child_nodes(body, path);
        assert_named_width(path, &body_children, "Left Rail", 64);
        assert_named_width(path, &body_children, "History Panel", 250);
        let workspace = named_child(&body_children, "Request Workspace").unwrap_or_else(|| {
            panic!(
                "{} application must contain Request Workspace",
                path.display()
            )
        });
        assert_eq!(
            workspace.get("width").and_then(Value::as_str),
            Some("fill_container"),
            "{} Request Workspace must fill remaining width",
            path.display()
        );
    }
}

fn assert_root_contains_children(path: &Path, root: &Value) {
    let children = child_nodes(root, path);
    let child_height = children
        .iter()
        .map(|child| number_field(child, "height", path))
        .sum::<i64>();
    let required = child_height
        + ROOT_GAP * i64::try_from(children.len().saturating_sub(1)).expect("section count")
        + ROOT_PADDING * 2;
    let actual = number_field(root, "height", path);
    assert!(
        actual >= required,
        "{} root height {actual}px clips sections requiring {required}px",
        path.display()
    );
}

fn assert_named_width(path: &Path, nodes: &[&Value], name: &str, expected: i64) {
    let node = named_child(nodes, name)
        .unwrap_or_else(|| panic!("{} application must contain {name}", path.display()));
    assert_eq!(
        number_field(node, "width", path),
        expected,
        "{} {name} width",
        path.display()
    );
}

fn named_child<'a>(nodes: &'a [&Value], name: &str) -> Option<&'a Value> {
    nodes
        .iter()
        .copied()
        .find(|node| node.get("name").and_then(Value::as_str) == Some(name))
}

fn issue_number(file_name: &str) -> Option<u32> {
    let stem = file_name.strip_suffix(".pen")?;
    let rest = stem.strip_prefix("issue-")?;
    let (digits, slug) = rest.split_once('-')?;
    if digits.len() != 4 || !digits.bytes().all(|byte| byte.is_ascii_digit()) || slug.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes[8] != b'-'
        || bytes[13] != b'-'
        || bytes[18] != b'-'
        || bytes[23] != b'-'
        || bytes[14] != b'4'
        || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
    {
        return false;
    }

    bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit())
}

fn collect_text(document: &Value) -> String {
    let mut content = Vec::new();
    walk(document, &mut |value| {
        if let Some(text) = value.get("content").and_then(Value::as_str) {
            content.push(text.to_string());
        }
    });
    content.join("\n")
}

fn walk(value: &Value, visitor: &mut impl FnMut(&Value)) {
    match value {
        Value::Object(object) => {
            visitor(value);
            for child in object.values() {
                walk(child, visitor);
            }
        }
        Value::Array(array) => {
            for child in array {
                walk(child, visitor);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn read_document(path: &Path) -> Value {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn single_root<'a>(path: &Path, document: &'a Value) -> &'a Value {
    let roots = document
        .get("children")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} must contain children", path.display()));
    assert_eq!(
        roots.len(),
        1,
        "{} must contain exactly one root frame",
        path.display()
    );
    &roots[0]
}

fn child_nodes<'a>(node: &'a Value, path: &Path) -> Vec<&'a Value> {
    node.get("children")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} node has no children", path.display()))
        .iter()
        .collect()
}

fn string_field<'a>(node: &'a Value, field: &str, path: &Path) -> &'a str {
    node.get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{} node is missing string field {field}", path.display()))
}

fn number_field(node: &Value, field: &str, path: &Path) -> i64 {
    node.get(field)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("{} node is missing numeric field {field}", path.display()))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
