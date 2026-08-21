use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const PENCIL_VERSION: &str = "2.17";
const ROOT_WIDTH: i64 = 1600;
const ROOT_PADDING: i64 = 32;
const ROOT_GAP: i64 = 24;
const APP_HEIGHT: i64 = 900;
const GITHUB_ISSUE_BASE: &str = "github.com/847850277/postman-gpui/issues";

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

struct IssueContentContract {
    issue: u32,
    required_node_names: &'static [&'static str],
    required_visible_text: &'static [&'static str],
}

// Static design validation cannot infer product meaning from arbitrary canvas nodes. Requiring
// issue-specific controls and visible values keeps a copied shell or explanatory-card-only design
// from satisfying the contract. These markers are intentionally scoped to the feature stage; text
// repeated only in the Design Header or E2E Contract does not count.
const ISSUE_CONTENT_CONTRACTS: &[IssueContentContract] = &[
    IssueContentContract {
        issue: 51,
        required_node_names: &[
            "Parameter Row · q",
            "Parameter Row · locale",
            "Effective URL Preview",
            "HTTPBingo Response Panel",
        ],
        required_visible_text: &[
            "existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87",
            "Ready to send — the active value is already in the ViewModel",
            "\"q\": [\"rust gpui\"]",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 52,
        required_node_names: &[
            "Headers Editor",
            "Enabled Header Row · X-Scenario",
            "Disabled Header Row · X-Disabled",
            "Response Panel",
        ],
        required_visible_text: &[
            "X-Scenario: httpbingo-headers",
            "X-Disabled: must-not-be-sent",
            "X-Scenario present · X-Disabled absent",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 53,
        required_node_names: &[
            "Token Input",
            "Before Send Content Item",
            "Generated Header Content Item",
            "Response Panel",
        ],
        required_visible_text: &[
            "Bearer scenario-token",
            "Authorization: Bearer scenario-token",
            "\"authenticated\": true",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 54,
        required_node_names: &[
            "Username Input",
            "Password Input Active",
            "Authorization Ready Row",
            "Response Panel",
        ],
        required_visible_text: &[
            "scenario-user",
            "Authorization: Basic c2NlbmFyaW8tdXNlcjpzY2VuYXJpby1wYXNz",
            "\"authenticated\": true",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 55,
        required_node_names: &[
            "Method Selection Editor",
            "Selected Method Content Item",
            "Outgoing Request Content Item",
            "DELETE History Result Content Item",
        ],
        required_visible_text: &[
            "DELETE https://httpbingo.org/delete · body = None",
            "\"method\": \"DELETE\"",
            "200 OK",
            "Copy reads the complete raw body from ResponseState",
        ],
    },
    IssueContentContract {
        issue: 56,
        required_node_names: &[
            "JSON Editor Active",
            "ViewModel Body Content Item",
            "Outgoing PATCH Content Item",
            "PATCH History Result Content Item",
        ],
        required_visible_text: &[
            "{\"patched\":true}",
            "RequestBody::Json",
            "Content-Type: application/json",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 57,
        required_node_names: &[
            "JSON Editor Active",
            "ViewModel Body Content Item",
            "Effective Headers Column",
            "POST JSON History Result Content Item",
        ],
        required_visible_text: &[
            "{\"name\":\"Ada\",\"active\":true}",
            "JSON · application/json",
            "X-Scenario",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 58,
        required_node_names: &[
            "URL-Encoded Form Editor",
            "Form Row · name",
            "Active Value Input · true",
            "Effective Request Preview",
        ],
        required_visible_text: &[
            "name=Ada+Lovelace&active=true",
            "application/x-www-form-urlencoded",
            "\"name\": [\"Ada Lovelace\"]",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 59,
        required_node_names: &[
            "HTML Form Submission Editor",
            "Form Row · comments active",
            "Effective Submission Preview",
            "History Result · GET Form Discovery",
        ],
        required_visible_text: &[
            "custname=Ada+Lovelace",
            "comments=Ring+the+bell",
            "\"topping\": [\"bacon\", \"cheese\"]",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 60,
        required_node_names: &[
            "Raw Body Editor Active",
            "ViewModel Raw Body Content Item",
            "No Generated Content-Type Content Item",
            "Effective Raw Request Preview",
        ],
        required_visible_text: &[
            "plain text body",
            "RequestBody::Raw(\"plain text body\")",
            "Content-Type: not generated",
            "data:application/octet-stream;base64,cGxhaW4gdGV4dCBib2R5",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 70,
        required_node_names: &[
            "Global Search Idle",
            "Search Results Popover",
            "Selected History Result",
            "Empty Results Popover",
        ],
        required_visible_text: &[
            "Search requests and history",
            "OPEN REQUESTS  ·  1",
            "↑↓ Navigate · ↵ Open · Esc Close",
            "No matching requests",
        ],
    },
    IssueContentContract {
        issue: 72,
        required_node_names: &[
            "Parameter Row · q",
            "Parameter Row · locale",
            "Parameter Row - limit (disabled)",
            "Effective URL Preview",
        ],
        required_visible_text: &[
            "8 rows stored in ViewModel",
            "existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87",
            "7 enabled / 1 disabled",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 74,
        required_node_names: &[
            "Copy Response Button",
            "Positive Evidence Content Item",
            "Negative Evidence Content Item",
            "Lifecycle Content Item",
        ],
        required_visible_text: &[
            "Clipboard equals the complete ResponseState body",
            "Not sent and empty states expose no active Copy action",
            "Copied",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 81,
        required_node_names: &[
            "Header Row - X-Scenario",
            "Header Row - X-Locale",
            "Header Row - X-Disabled",
            "Header Persistence and Send Projection",
        ],
        required_visible_text: &[
            "active values already live in ViewModel",
            "Sent: X-Scenario, X-Locale",
            "X-Disabled is not present",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 91,
        required_node_names: &[
            "Multipart Text Editor",
            "Multipart Text Row · note",
            "Active Value Input · gpui",
            "Effective Multipart Preview",
        ],
        required_visible_text: &[
            "Text(note = hello multipart)  ·  Text(category = gpui)",
            "multipart/form-data; boundary=<generated>",
            "\"category\": [\"gpui\"]",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 92,
        required_node_names: &[
            "Multipart File Upload Editor",
            "Multipart File Row · upload",
            "Replace File Button",
            "Effective File Upload Preview",
        ],
        required_visible_text: &[
            "httpbingo-upload.txt",
            "File(upload = httpbingo-upload.txt · text/plain)",
            "files.upload matches repository fixture",
            "200 OK",
        ],
    },
    IssueContentContract {
        issue: 93,
        required_node_names: &[
            "Multipart Safety Editor",
            "Multipart Row · ignored",
            "Multipart Row · upload",
            "Effective Safe Request Preview",
        ],
        required_visible_text: &[
            "Saved in ViewModel; excluded from request",
            "request not executed · no successful History entry",
            "\"response_state\": \"error\"",
            "NOT SENT",
        ],
    },
    IssueContentContract {
        issue: 95,
        required_node_names: &[
            "Multiple URL-Encoded Rows Editor",
            "URL-Encoded Row · tag rust",
            "URL-Encoded Row · ignored",
            "Fixed Effective Request Preview",
        ],
        required_visible_text: &[
            "name=Ada+Lovelace&active=true&tag=rust&tag=gpui",
            "ignored and empty keys are absent",
            "\"tag\": [\"rust\", \"gpui\"]",
            "200 OK",
        ],
    },
];

#[test]
fn issue_design_artifacts_follow_the_shared_contract() {
    let design_dir = manifest_dir().join("design");
    let readme = fs::read_to_string(design_dir.join("README.md"))
        .expect("design/README.md should be readable");
    let spec =
        fs::read_to_string(design_dir.join("SPEC.md")).expect("design/SPEC.md should be readable");
    for section in [
        "## 3. Canvas and Application Shell",
        "## 4. Content Items",
        "## 5. Interaction States",
        "## 6. E2E Contract Section",
        "## 7. Shared Tokens",
        "## 8. Mapping and Traceability",
    ] {
        assert!(
            spec.contains(section),
            "SPEC.md must retain normative section {section}"
        );
    }

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

    let artifact_issues = artifacts
        .iter()
        .map(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("design filename should be UTF-8");
            issue_number(file_name)
                .unwrap_or_else(|| panic!("{file_name} does not match issue-NNNN-feature.pen"))
        })
        .collect::<HashSet<_>>();
    let contracted_issues = ISSUE_CONTENT_CONTRACTS
        .iter()
        .map(|contract| contract.issue)
        .collect::<HashSet<_>>();
    assert_eq!(
        artifact_issues, contracted_issues,
        "every issue artifact must have exactly one issue-specific content contract"
    );
    assert_eq!(
        artifacts.len(),
        artifact_issues.len(),
        "an issue must not own multiple design artifacts"
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
    assert_tokenized_colors(&path, root);

    let all_text = collect_text(&document);
    assert_httpbingo_endpoint(&path, &all_text);
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
    assert_tokenized_colors(path, root);

    let root_name = string_field(root, "name", path);
    assert!(
        root_name.contains(&format!("Issue #{issue}")),
        "{} root name must contain Issue #{}",
        path.display(),
        issue
    );
    if let Some(context) = root.get("context").and_then(Value::as_str) {
        assert!(
            context.contains(&format!("GitHub Issue #{issue}")),
            "{} root context must identify its owning GitHub Issue #{issue}, got {context:?}",
            path.display()
        );
    }

    let sections = child_nodes(root, path);
    let header = sections[0];
    let contract = sections
        .last()
        .copied()
        .expect("a validated root has an E2E Contract");
    let feature_stages = &sections[1..sections.len() - 1];
    assert_design_header(path, header, issue);
    assert_feature_content(path, feature_stages, issue);
    assert_e2e_contract(path, contract, issue, file_name);

    let all_text = collect_text(&document);
    assert_httpbingo_endpoint(path, &all_text);
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
        readme.contains(&format!("[#{issue} "))
            && readme.contains(file_name)
            && readme.contains(&format!("https://{GITHUB_ISSUE_BASE}/{issue}")),
        "design/README.md must map Issue #{} to {}",
        issue,
        file_name
    );
}

fn assert_design_header(path: &Path, header: &Value, issue: u32) {
    assert_eq!(
        header.get("width").and_then(Value::as_str),
        Some("fill_container"),
        "{} Design Header must fill the root",
        path.display()
    );

    let header_text = collect_text(header);
    assert!(
        header_text.contains(&format!("#{issue}")),
        "{} Design Header must show Issue #{issue}",
        path.display()
    );

    let issue_url = find_descendant(header, &|node| {
        node_name(node).is_some_and(|name| name.to_ascii_lowercase().contains("issue url"))
    })
    .unwrap_or_else(|| panic!("{} Design Header is missing Issue URL", path.display()));
    let issue_url_text = collect_text(issue_url);
    assert!(
        issue_url_text.contains(&format!("{GITHUB_ISSUE_BASE}/{issue}")),
        "{} Design Header has the wrong Issue URL: {issue_url_text:?}",
        path.display()
    );

    let title = find_descendant(header, &|node| {
        node_name(node).is_some_and(|name| name == "Design Title")
    })
    .unwrap_or_else(|| panic!("{} Design Header is missing Design Title", path.display()));
    let title_text = collect_text(title);
    assert!(
        !title_text.trim().is_empty() && !title_text.eq_ignore_ascii_case("feature title"),
        "{} Design Header must use a real feature title",
        path.display()
    );

    let status = find_descendant(header, &|node| {
        node_name(node).is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("status") || name.contains("baseline")
        })
    })
    .unwrap_or_else(|| {
        panic!(
            "{} Design Header is missing delivery status",
            path.display()
        )
    });
    assert!(
        !collect_text(status).trim().is_empty(),
        "{} Design Header delivery status must be visible text",
        path.display()
    );

    let endpoint_or_scope = find_descendant(header, &|node| {
        node_name(node).is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("endpoint") || name.contains("scope")
        })
    })
    .unwrap_or_else(|| {
        panic!(
            "{} Design Header is missing endpoint or scope",
            path.display()
        )
    });
    assert!(
        !collect_text(endpoint_or_scope).trim().is_empty(),
        "{} Design Header endpoint or scope must be visible text",
        path.display()
    );

    let summary = find_descendant(header, &|node| {
        node_name(node).is_some_and(|name| name == "Design Summary")
    })
    .unwrap_or_else(|| panic!("{} Design Header is missing Design Summary", path.display()));
    let summary_text = collect_text(summary);
    let summary_text = summary_text.trim();
    assert!(
        summary_text.len() >= 30
            && !summary_text.contains('\n')
            && summary_text
                .chars()
                .last()
                .is_some_and(|character| matches!(character, '.' | '!' | '?')),
        "{} Design Summary must be one complete sentence, got {summary_text:?}",
        path.display()
    );
}

fn assert_feature_content(path: &Path, stages: &[&Value], issue: u32) {
    let contract = ISSUE_CONTENT_CONTRACTS
        .iter()
        .find(|contract| contract.issue == issue)
        .unwrap_or_else(|| panic!("Issue #{issue} is missing an issue-specific content contract"));
    let stage_text = normalize_whitespace(
        &stages
            .iter()
            .map(|stage| collect_text(stage))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut stage_names = Vec::new();
    for stage in stages {
        collect_names(stage, &mut stage_names);
    }

    for required in contract.required_node_names {
        assert!(
            stage_names.iter().any(|name| name.contains(required)),
            "{} feature stage is missing concrete node {required:?}",
            path.display()
        );
    }
    for required in contract.required_visible_text {
        let required = normalize_whitespace(required);
        assert!(
            stage_text.contains(&required),
            "{} feature stage is missing visible contract value {required:?}",
            path.display()
        );
    }

    let stage_text_lower = stage_text.to_ascii_lowercase();
    assert!(
        [
            "active",
            "enabled",
            "selected",
            "disabled",
            "dirty",
            "valid",
            "ready",
            "before send",
        ]
        .iter()
        .any(|state| {
            stage_text_lower.contains(state)
                || stage_names
                    .iter()
                    .any(|name| name.to_ascii_lowercase().contains(state))
        }),
        "{} feature content must visibly identify the current input state",
        path.display()
    );
    if !matches!(issue, 70 | 74) {
        let outgoing = stages.iter().find_map(|stage| {
            find_descendant(stage, &|node| {
                node_name(node).is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.contains("effective")
                        || name.contains("outgoing")
                        || name.contains("request preview")
                        || name.contains("network step")
                        || name.contains("request state step")
                        || name.contains("generated header")
                        || name.contains("authorization ready")
                        || name.contains("send projection")
                })
            })
        });
        let outgoing = outgoing.unwrap_or_else(|| {
            panic!(
                "{} feature content must show the final outgoing request representation",
                path.display()
            )
        });
        assert!(
            !collect_text(outgoing).trim().is_empty(),
            "{} outgoing request representation must contain visible values",
            path.display()
        );
    }

    if issue != 70 {
        assert_response_evidence(path, &stage_text_lower, &stage_names);
    }

    let mut responses = Vec::new();
    for stage in stages {
        collect_descendants(stage, &mut responses, &|node| {
            node_name(node).is_some_and(|name| {
                name.contains("Response Card") || name.contains("Response Panel")
            })
        });
    }
    responses.retain(|response| response_status_node(response).is_some());
    assert!(
        !responses.is_empty(),
        "{} feature content must contain a concrete Response Panel",
        path.display()
    );
    for response in responses {
        assert_response_content(path, response);
    }

    let mut application_states = 0;
    let mut state_cards = 0;
    for stage in stages {
        let name = string_field(stage, "name", path);
        if let Some(suffix) = name.strip_prefix("Application State") {
            application_states += 1;
            let feature_name = suffix.trim_matches(|character: char| {
                character.is_whitespace() || matches!(character, '·' | '-' | '–' | '—')
            });
            assert!(
                !feature_name.is_empty()
                    && !feature_name.eq_ignore_ascii_case("feature")
                    && !feature_name.contains('<'),
                "{} must replace the Application State feature placeholder, got {name:?}",
                path.display()
            );
            assert_full_application_content(path, stage);
        } else if name.starts_with("State Row") {
            for card in child_nodes(stage, path) {
                state_cards += 1;
                assert_state_card_content(path, card);
            }
        }
    }
    assert!(
        application_states > 0 || state_cards >= 2,
        "{} must contain a full Application State or a multi-state matrix",
        path.display()
    );
}

fn assert_full_application_content(path: &Path, app: &Value) {
    let app_children = child_nodes(app, path);
    let body = app_children
        .iter()
        .copied()
        .find(|node| node_name(node).is_some_and(|name| name.contains("Body")))
        .unwrap_or_else(|| panic!("{} application must contain a body", path.display()));
    let body_children = child_nodes(body, path);
    let history = named_child(&body_children, "History Panel")
        .unwrap_or_else(|| panic!("{} application must contain History Panel", path.display()));
    let workspace = named_child(&body_children, "Request Workspace").unwrap_or_else(|| {
        panic!(
            "{} application must contain Request Workspace",
            path.display()
        )
    });

    let history_result = find_descendant(history, &|node| {
        node_name(node).is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("history result")
                || name.contains("history item")
                || name.starts_with("history ·")
                || name.starts_with("history -")
        })
    })
    .unwrap_or_else(|| panic!("{} must show a relevant History result", path.display()));
    assert!(
        !collect_text(history_result).trim().is_empty(),
        "{} History result must contain observable values",
        path.display()
    );

    assert!(
        find_descendant(workspace, &|node| {
            node_name(node).is_some_and(|name| name.to_ascii_lowercase().contains("request tab"))
        })
        .is_some(),
        "{} Request Workspace is missing request tab",
        path.display()
    );
    assert!(
        find_descendant(workspace, &|node| {
            node_name(node).is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                name.contains("request builder")
                    || name.contains("request bar")
                    || name.contains("request head")
            })
        })
        .is_some(),
        "{} Request Workspace is missing request builder",
        path.display()
    );

    assert!(
        find_descendant(workspace, &|node| {
            node_name(node).is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                name.contains("editor") || name.contains("request panel")
            })
        })
        .is_some(),
        "{} Request Workspace is missing a concrete feature editor",
        path.display()
    );

    let response = find_descendant(workspace, &|node| {
        node_name(node).is_some_and(|name| {
            (name.contains("Response Panel") || name.contains("Response Card"))
                && response_status_node(node).is_some()
        })
    })
    .unwrap_or_else(|| {
        panic!(
            "{} Request Workspace is missing Response Panel",
            path.display()
        )
    });
    assert_response_content(path, response);
}

fn assert_state_card_content(path: &Path, card: &Value) {
    let name = string_field(card, "name", path);
    assert!(
        name.starts_with("State "),
        "{} state-matrix card needs a numbered State name, got {name:?}",
        path.display()
    );
    for required in ["State Title", "State Note"] {
        let node = find_descendant(card, &|node| node_name(node) == Some(required))
            .unwrap_or_else(|| panic!("{} {name} is missing {required}", path.display()));
        assert!(
            !collect_text(node).trim().is_empty(),
            "{} {name} {required} must contain visible text",
            path.display()
        );
    }
    assert!(
        find_descendant(card, &|node| {
            node_name(node).is_some_and(|node_name| {
                node_name.contains("App Preview") || node_name.contains("App Header")
            })
        })
        .is_some(),
        "{} {name} must show its owning application context",
        path.display()
    );
}

fn assert_response_content(path: &Path, response: &Value) {
    let status = response_status_node(response)
        .unwrap_or_else(|| panic!("{} Response Panel is missing status", path.display()));
    let status_text = collect_text(status);
    let copy_action = find_descendant(response, &|node| {
        node_name(node).is_some_and(|name| name.contains("Copy Response"))
    });

    if status_text.to_ascii_uppercase().contains("NOT SENT") {
        assert!(
            copy_action.is_none(),
            "{} Not sent response must not expose Copy",
            path.display()
        );
    } else {
        assert!(
            contains_http_status(&status_text),
            "{} populated Response Panel must show an HTTP status, got {status_text:?}",
            path.display()
        );
        assert!(
            copy_action.is_some(),
            "{} populated Response Panel must expose Copy",
            path.display()
        );

        let copy_text = collect_text(copy_action.expect("Copy existence was asserted"));
        assert!(
            copy_text.split_whitespace().any(|word| word == "Copy"),
            "{} Copy action must visibly identify its behavior",
            path.display()
        );
    }

    let response_text = collect_text(response);
    assert!(
        response_text.len() >= 20,
        "{} Response Panel must show concrete result evidence",
        path.display()
    );
}

fn response_status_node(response: &Value) -> Option<&Value> {
    find_descendant(response, &|node| {
        node_name(node).is_some_and(|name| name.to_ascii_lowercase().contains("status")) && {
            let text = collect_text(node);
            contains_http_status(&text) || text.to_ascii_uppercase().contains("NOT SENT")
        }
    })
}

fn assert_response_evidence(path: &Path, stage_text_lower: &str, stage_names: &[&str]) {
    assert!(
        ["stable", "echo", "assertion", "verification"]
            .iter()
            .any(|marker| {
                stage_text_lower.contains(marker)
                    || stage_names
                        .iter()
                        .any(|name| name.to_ascii_lowercase().contains(marker))
            }),
        "{} feature content must label the stable response assertion subset",
        path.display()
    );
    // The issue-specific visible-value contract above supplies the positive and, where
    // applicable, negative response evidence. This shared check verifies that those values are
    // explicitly framed as the stable assertion subset rather than as decorative sample data.
    let lifecycle_markers = [
        "responsestate",
        "response state",
        "response_state",
        "view",
        "history",
    ]
    .iter()
    .filter(|marker| stage_text_lower.contains(**marker))
    .count();
    let structural_lifecycle = ["lifecycle", "response", "history"].iter().all(|marker| {
        stage_names
            .iter()
            .any(|name| name.to_ascii_lowercase().contains(marker))
    });
    assert!(
        lifecycle_markers >= 2 || structural_lifecycle,
        "{} feature content must visibly connect ResponseState, View, and History",
        path.display()
    );
}

fn assert_e2e_contract(path: &Path, contract: &Value, issue: u32, file_name: &str) {
    let contract_text = collect_text(contract);
    let contract_text_lower = contract_text.to_ascii_lowercase();
    let mut contract_names = Vec::new();
    collect_names(contract, &mut contract_names);

    assert!(
        contract_text.contains(&format!("#{issue}")),
        "{} E2E Contract must identify Issue #{issue}",
        path.display()
    );
    assert!(
        contract_text.contains(file_name),
        "{} E2E Contract must contain its owning design path",
        path.display()
    );
    assert!(
        ["parent", "roadmap", "child of"]
            .iter()
            .any(|marker| contract_text_lower.contains(marker)),
        "{} E2E Contract must identify its parent or roadmap issue",
        path.display()
    );
    assert!(
        contract_names.iter().any(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("scenario") || name.contains("interaction contract")
        }),
        "{} E2E Contract must contain ordered real-UI scenario steps",
        path.display()
    );
    assert!(
        contract_names.iter().any(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("assertion") || name.contains("verification contract")
        }),
        "{} E2E Contract must contain observable assertions",
        path.display()
    );
    assert!(
        contract_text_lower.contains("included") || contract_text_lower.contains("in scope"),
        "{} E2E Contract must state included scope",
        path.display()
    );
    assert!(
        ["excluded", "out of scope", "not in this slice", "non-goal"]
            .iter()
            .any(|marker| contract_text_lower.contains(marker)),
        "{} E2E Contract must state explicit non-goals",
        path.display()
    );
    let one_to_one = find_descendant(contract, &|node| {
        node_name(node).is_some_and(|name| name == "One-to-One Rule")
    })
    .unwrap_or_else(|| panic!("{} E2E Contract is missing One-to-One Rule", path.display()));
    let one_to_one_text = collect_text(one_to_one);
    assert!(
        one_to_one_text.to_ascii_uppercase().contains("ONE") && one_to_one_text.contains(file_name),
        "{} One-to-One Rule must bind one issue to {file_name}",
        path.display()
    );

    let test_paths = extract_test_paths(&contract_text);
    assert!(
        !test_paths.is_empty(),
        "{} E2E Contract must contain a scenario or test path",
        path.display()
    );
    assert!(
        test_paths
            .iter()
            .any(|test_path| test_path.ends_with(".rs") || test_path.ends_with(".json")),
        "{} E2E Contract must reference an executable Rust test or JSON scenario",
        path.display()
    );
    for test_path in test_paths {
        assert!(
            !test_path
                .chars()
                .any(|character| matches!(character, '*' | '?' | '{' | '[')),
            "{} E2E Contract path must be concrete, got {test_path:?}",
            path.display()
        );
        assert!(
            manifest_dir().join(&test_path).is_file(),
            "{} E2E Contract references missing file {test_path:?}",
            path.display()
        );
    }
}

fn assert_tokenized_colors(path: &Path, root: &Value) {
    inspect_color_values(path, root, false);
}

fn inspect_color_values(path: &Path, value: &Value, inside_effect: bool) {
    match value {
        Value::Object(object) => {
            if !inside_effect {
                for field in ["fill", "stroke", "color"] {
                    if let Some(color) = object.get(field).and_then(Value::as_str) {
                        assert!(
                            !color.starts_with('#'),
                            "{} node {:?} uses one-off {field} color {color}; use a shared token",
                            path.display(),
                            object
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("<unnamed>")
                        );
                    }
                }
            }
            for (field, child) in object {
                inspect_color_values(path, child, inside_effect || field == "effect");
            }
        }
        Value::Array(array) => {
            for child in array {
                inspect_color_values(path, child, inside_effect);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn extract_test_paths(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let start = word.find("tests/")?;
            let mut path = word[start..]
                .trim_matches(|character: char| {
                    matches!(character, '`' | '"' | '\'' | '(' | '[' | '{')
                })
                .to_string();
            while path.chars().last().is_some_and(|character| {
                matches!(character, '.' | ',' | ';' | ':' | ')' | ']' | '}')
            }) {
                path.pop();
            }
            (!path.is_empty()).then_some(path)
        })
        .collect()
}

fn contains_http_status(text: &str) -> bool {
    text.as_bytes().windows(3).any(|window| {
        matches!(window[0], b'1'..=b'5') && window[1].is_ascii_digit() && window[2].is_ascii_digit()
    })
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn node_name(node: &Value) -> Option<&str> {
    node.get("name").and_then(Value::as_str)
}

fn find_descendant<'a>(value: &'a Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a Value> {
    if predicate(value) {
        return Some(value);
    }
    match value {
        Value::Object(object) => object
            .values()
            .find_map(|child| find_descendant(child, predicate)),
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_descendant(child, predicate)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

fn collect_descendants<'a>(
    value: &'a Value,
    matches: &mut Vec<&'a Value>,
    predicate: &impl Fn(&Value) -> bool,
) {
    if predicate(value) {
        matches.push(value);
    }
    match value {
        Value::Object(object) => {
            for child in object.values() {
                collect_descendants(child, matches, predicate);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_descendants(child, matches, predicate);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn collect_names<'a>(value: &'a Value, names: &mut Vec<&'a str>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object.get("name").and_then(Value::as_str) {
                names.push(name);
            }
            for child in object.values() {
                collect_names(child, names);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_names(child, names);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn assert_httpbingo_endpoint(path: &Path, all_text: &str) {
    const LEGACY_HTTPBIN_HOST: &str = concat!("httpbin", ".org");

    assert!(
        all_text.contains("https://httpbingo.org"),
        "{} must use an HTTPS HTTPBingo endpoint",
        path.display()
    );
    assert!(
        !all_text.contains(LEGACY_HTTPBIN_HOST),
        "{} must not use the legacy HTTPBin host",
        path.display()
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
