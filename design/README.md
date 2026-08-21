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
| [#55 DELETE Request](https://github.com/847850277/postman-gpui/issues/55) | [`issue-0055-delete-request.pen`](issue-0055-delete-request.pen) |
| [#56 PATCH JSON](https://github.com/847850277/postman-gpui/issues/56) | [`issue-0056-patch-json.pen`](issue-0056-patch-json.pen) |
| [#57 JSON Body and Automatic Content-Type](https://github.com/847850277/postman-gpui/issues/57) | [`issue-0057-json-body-content-type.pen`](issue-0057-json-body-content-type.pen) |
| [#58 URL-Encoded Form](https://github.com/847850277/postman-gpui/issues/58) | [`issue-0058-url-encoded-form.pen`](issue-0058-url-encoded-form.pen) |
| [#59 HTML Form Page and Submission](https://github.com/847850277/postman-gpui/issues/59) | [`issue-0059-html-form-submission.pen`](issue-0059-html-form-submission.pen) |
| [#60 Raw Request Body](https://github.com/847850277/postman-gpui/issues/60) | [`issue-0060-raw-request-body.pen`](issue-0060-raw-request-body.pen) |
| [#61 Non-2xx Response](https://github.com/847850277/postman-gpui/issues/61) | [`issue-0061-non-2xx-response.pen`](issue-0061-non-2xx-response.pen) |
| [#62 Redirect Following](https://github.com/847850277/postman-gpui/issues/62) | [`issue-0062-redirect-following.pen`](issue-0062-redirect-following.pen) |
| [#63 JSON Response](https://github.com/847850277/postman-gpui/issues/63) | [`issue-0063-json-response.pen`](issue-0063-json-response.pen) |
| [#65 Cookie Storage, Sending, and Clearing](https://github.com/847850277/postman-gpui/issues/65) | [`issue-0065-cookie-storage-sending-clearing.pen`](issue-0065-cookie-storage-sending-clearing.pen) |
| [#66 Delayed Requests, Cancellation, and Timeout](https://github.com/847850277/postman-gpui/issues/66) | [`issue-0066-delay-cancel-timeout.pen`](issue-0066-delay-cancel-timeout.pen) |
| [#70 Global Search: Requests and History](https://github.com/847850277/postman-gpui/issues/70) | [`issue-0070-global-search.pen`](issue-0070-global-search.pen) |
| [#72 Multiple Query Parameter Rows](https://github.com/847850277/postman-gpui/issues/72) | [`issue-0072-multiple-query-parameter-rows.pen`](issue-0072-multiple-query-parameter-rows.pen) |
| [#74 Quick Copy for Populated Response Bodies](https://github.com/847850277/postman-gpui/issues/74) | [`issue-0074-response-quick-copy.pen`](issue-0074-response-quick-copy.pen) |
| [#81 Multiple Header Rows](https://github.com/847850277/postman-gpui/issues/81) | [`issue-0081-multiple-header-rows.pen`](issue-0081-multiple-header-rows.pen) |
| [#91 Multipart Text Rows](https://github.com/847850277/postman-gpui/issues/91) | [`issue-0091-multipart-text-rows.pen`](issue-0091-multipart-text-rows.pen) |
| [#92 Multipart File Picker and Upload](https://github.com/847850277/postman-gpui/issues/92) | [`issue-0092-multipart-file-picker-upload.pen`](issue-0092-multipart-file-picker-upload.pen) |
| [#93 Multipart Disabled Rows and Invalid Files](https://github.com/847850277/postman-gpui/issues/93) | [`issue-0093-multipart-disabled-invalid-files.pen`](issue-0093-multipart-disabled-invalid-files.pen) |
| [#95 Multiple URL-Encoded Form Rows](https://github.com/847850277/postman-gpui/issues/95) | [`issue-0095-multiple-urlencoded-form-rows.pen`](issue-0095-multiple-urlencoded-form-rows.pen) |
