# postman-flow-mcp

`postman-flow-mcp` lets MCP clients create, inspect, and compile `postman-flow` YAML documents
without copying secrets into prompts or executing network requests. Agent-generated documents are
accepted as structured JSON, compiled by `postman-flow`, and only then written as canonical YAML.

The document Schema is maintained by the engine at
[`postman-flow/flow-v1.schema.json`](../postman-flow/flow-v1.schema.json) and embedded through
`postman_flow::FLOW_DOCUMENT_SCHEMA_JSON`. This crate does not maintain a separate copy.

## Tools

- `get_dsl_schema`: exact Flow v1 JSON Schema plus generation rules.
- `get_dsl_reference`: full DSL reference from `postman-flow`.
- `list_flow_examples`: validated minimal and API catalog examples.
- `validate_flow`: parse and compile a relative file, JSON document, or YAML draft.
- `inspect_flow`: return inputs, ordered steps, checks, exports, and outputs.
- `create_flow`: compile a JSON document and save canonical YAML.

The server never sends HTTP requests. Every file operation is confined to `--root`; absolute paths,
parent traversal, unsupported extensions, and symlinked paths are rejected. Existing files are not
replaced unless `overwrite: true` is passed to `create_flow`.

## Build and run

```bash
cargo build --locked -p postman-flow-mcp

cargo run --locked -p postman-flow-mcp -- \
  --root /absolute/path/to/your/flow-project \
  --max-steps 256
```

The server uses MCP stdio transport, so stdout is reserved for protocol messages. Logs go to stderr.

Example MCP client configuration after a release build:

```json
{
  "mcpServers": {
    "postman-flow": {
      "command": "/absolute/path/to/postman-gpui/target/release/postman-flow-mcp",
      "args": ["--root", "/absolute/path/to/flow-project"]
    }
  }
}
```

Recommended Agent sequence:

1. Call `get_dsl_schema` or `list_flow_examples`.
2. Build a complete JSON document with `schema_version: 1`.
3. Call `validate_flow` with the structured `document`.
4. Correct any parser or compiler diagnostics.
5. Call `create_flow` to save the canonical `.http.yml` file.
