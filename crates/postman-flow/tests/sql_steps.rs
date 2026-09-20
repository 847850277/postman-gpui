#![cfg(feature = "sql")]
mod support;
use postman_flow::*;
use support::{run, FakeTransport};

#[tokio::test]
async fn test_sql_step_execution_and_exports() {
    let yaml = r#"
schema_version: 1
flow:
  name: test-sql-flow
  inputs:
  - name: db_url
    default: "sqlite:///tmp/test_sql_unit.db"
  steps:
  - id: step-create
    name: Create Table
    request:
      kind: sql
      connection:
        input: db_url
      query:
        literal: "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);"
    checks:
    - kind: status
      equals: 200

  - id: step-insert
    name: Insert Item
    request:
      kind: sql
      connection:
        input: db_url
      query:
        literal: "INSERT INTO items (name) VALUES (?);"
      params:
      - literal: "Widget"
    checks:
    - kind: status
      equals: 200
    - kind: jsonpath
      path: $.rows_affected
      equals:
        literal: 1
    exports:
    - name: inserted_id
      path: $.last_insert_id

  - id: step-select
    name: Select Item
    request:
      kind: sql
      connection:
        input: db_url
      query:
        literal: "SELECT id, name FROM items WHERE name = ?;"
      params:
      - literal: "Widget"
    checks:
    - kind: status
      equals: 200
    - kind: jsonpath
      path: $.count
      equals:
        literal: 1
    - kind: jsonpath
      path: $.rows[0].name
      equals:
        literal: "Widget"
    exports:
    - name: found_name
      path: $.rows[0].name
"#;

    let doc = parse_flow_yaml(yaml).expect("YAML should parse");
    let plan = compile_flow(&doc.flow, &doc.apis, &CompileEnvironment::default())
        .expect("flow should compile");

    assert_eq!(plan.step_count(), 3);

    let transport = FakeTransport::new([]);
    let events = run(plan, FlowInputs::default(), transport).await;
    for e in &events {
        println!("Event: {:?}", e);
    }

    // Check that all steps finished with Succeeded
    let finished_steps: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            FlowEvent::StepFinished { step_id, outcome } => {
                Some((step_id.as_str(), outcome == &StepOutcome::Succeeded))
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        finished_steps,
        vec![
            ("step-create", true),
            ("step-insert", true),
            ("step-select", true)
        ]
    );

    // Check exported output
    let exported: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            FlowEvent::OutputExported { step_id, name } => Some((step_id.as_str(), name.as_str())),
            _ => None,
        })
        .collect();

    assert_eq!(
        exported,
        vec![("step-insert", "inserted_id"), ("step-select", "found_name")]
    );

    let _ = std::fs::remove_file("/tmp/test_sql_unit.db");
}
