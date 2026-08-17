# E2E Design Artifacts

Each UI-function E2E issue owns exactly one Pencil design file.

## Naming

```text
issue-<four-digit-issue-number>-<feature-slug>.pen
```

Example:

```text
Issue #51 -> issue-0051-query-parameter-encoding.pen
```

## Rules

- Keep one E2E feature slice in one `.pen` file; do not combine unrelated issues into a large design file.
- Include both the implementable UI state and the request/response interaction contract in the file.
- Add a `Design Artifact` link to the corresponding GitHub issue.
- Update the existing issue file when the slice changes instead of creating untracked variants.

## Current Mapping

| E2E issue | Design file |
| --- | --- |
| [#51 Query Parameter Encoding](https://github.com/847850277/postman-gpui/issues/51) | [`issue-0051-query-parameter-encoding.pen`](issue-0051-query-parameter-encoding.pen) |
