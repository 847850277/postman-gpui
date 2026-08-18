# E2E Design Artifacts

Each UI-function E2E issue owns exactly one Pencil design file.

The normative layout, content-item, token, traceability, and validation rules
are defined in [`SPEC.md`](SPEC.md). Start new work from
[`templates/e2e-feature.pen`](templates/e2e-feature.pen).

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
- Run `cargo test --test design_artifacts` before handing off a design change.

## Current Mapping

| E2E issue | Design file |
| --- | --- |
| [#51 Query Parameter Encoding](https://github.com/847850277/postman-gpui/issues/51) | [`issue-0051-query-parameter-encoding.pen`](issue-0051-query-parameter-encoding.pen) |
| [#52 Custom and Disabled Headers](https://github.com/847850277/postman-gpui/issues/52) | [`issue-0052-custom-disabled-headers.pen`](issue-0052-custom-disabled-headers.pen) |
| [#53 Bearer Auth](https://github.com/847850277/postman-gpui/issues/53) | [`issue-0053-bearer-auth.pen`](issue-0053-bearer-auth.pen) |
| [#54 Basic Auth](https://github.com/847850277/postman-gpui/issues/54) | [`issue-0054-basic-auth.pen`](issue-0054-basic-auth.pen) |
| [#70 Global Search: Requests and History](https://github.com/847850277/postman-gpui/issues/70) | [`issue-0070-global-search.pen`](issue-0070-global-search.pen) |
| [#72 Multiple Query Parameter Rows](https://github.com/847850277/postman-gpui/issues/72) | [`issue-0072-multiple-query-parameter-rows.pen`](issue-0072-multiple-query-parameter-rows.pen) |
| [#74 Quick Copy for Populated Response Bodies](https://github.com/847850277/postman-gpui/issues/74) | [`issue-0074-response-quick-copy.pen`](issue-0074-response-quick-copy.pen) |
| [#81 Multiple Header Rows](https://github.com/847850277/postman-gpui/issues/81) | [`issue-0081-multiple-header-rows.pen`](issue-0081-multiple-header-rows.pen) |
