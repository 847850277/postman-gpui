# Live editor synchronization audit

Issue [#49](https://github.com/847850277/postman-gpui/issues/49) uses “auto-fill” to mean that
every request editor writes through to the active request tab while the user types. It does not
mean suggestion or completion UI.

## Acceptance contract

- Typing, deleting, cutting, and pasting update the active tab's `WorkspaceViewModel` immediately.
- Enter, Tab, blur, Add, and Send are navigation or actions, not commit boundaries.
- Send reads the already synchronized model and does not backfill values from focused controls.
- Switching tabs preserves independent drafts, including disabled and blank rows.

## Automated evidence

| #49 surface | Regression coverage |
| --- | --- |
| URL | `get_418_is_a_completed_response_with_exact_view_and_history_status`; `pasting_a_complete_query_url_populates_params_and_sends_each_pair_once` |
| Params | `query_parameters_merge_encode_and_send_without_focus_change`; `multiple_query_rows_can_be_created_before_editing_and_sent` |
| Headers | `header_is_saved_before_add_or_focus_change`; `multiple_header_rows_can_be_created_before_editing_and_sent` |
| Bearer and Basic authorization | `bearer_authorization_editor_affects_the_real_request`; `basic_authorization_editor_affects_the_real_request` |
| JSON and Raw bodies | `post_json_merges_generated_headers_with_a_custom_row_and_sends_the_active_value`; `put_raw_sends_active_exact_body_without_generated_content_type_and_records_history` |
| URL-encoded and multipart bodies | `post_urlencoded_sends_the_active_value_and_excludes_disabled_rows`; `multipart_text_rows_are_typed_live_and_sent_without_committing_the_active_cell` |
| Scripts and Tests | `script_and_test_editors_are_saved_per_tab` |
| Cut, paste, deletion, and keyboard navigation | `text_editing_shortcuts_remain_local_and_projection_safe`; `option_groups_and_dynamic_rows_are_fully_keyboard_operable` |
| Tab isolation and draft restoration | `every_composer_pane_restores_active_edits_for_its_request_tab`; `row_editors_project_independent_pane_and_tab_drafts` |

The named tests live in `tests/ui_send.rs`, `tests/ui_workspace.rs`, and `tests/ui_keyboard.rs`.
They exercise the rendered controls and inspect the model and, where applicable, the real HTTP
request. The active-field scenarios deliberately click Send without Enter, Tab, blur, or Add.

Run the complete contract from a clean checkout with:

```bash
cargo test --locked --all-targets --all-features
```

## Release decision

The code-level acceptance criteria for #49 are satisfied when the suite above passes. Close #49
after these tests and this audit are merged into the release branch; keep the clean-install manual
check in `docs/release-smoke-test.md` as the final package-level confirmation.
