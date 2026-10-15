//! Retained response document. Kit owns layout, selection, highlighting and both scroll axes.
use crate::{
    app::ResponseState,
    models::HistoricalResponseBody,
    ui::{
        components::common::edit_context_menu::edit_popup_menu,
        theme::{
            metrics as m, ColorToken, CODE_KEY, CODE_NUMBER, CODE_STRING, FONT_MONO, MUTED,
            SIDEBAR, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, relative, rems, App, AppContext, Context, Entity, FocusHandle,
    Focusable, HighlightStyle, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window,
};
use gpui_kit::component::{
    input::{
        Copy, Editor, EditorState, Escape, SelectAll, TextDecoration, TextDecorationCollection,
    },
    menu::ContextMenuExt,
    Theme,
};
use std::{ops::Range, sync::Arc};

pub(super) struct ResponseBody {
    pub(super) editor: Entity<EditorState>,
    response: Option<Arc<ResponseState>>,
    generation: u64,
    preparation: Option<Task<()>>,
    pretty: bool,
    has_payload: bool,
    syntax: Vec<(Range<usize>, ColorToken)>,
    decorations: TextDecorationCollection,
    _theme_subscription: Subscription,
}

impl ResponseBody {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .context_menu(false)
                .searchable(false)
                .folding(false)
                .line_number(false)
                .scroll_beyond_last_line(Some(0));
            editor.set_readonly(true, cx);
            editor
        });
        let decorations = editor.update(cx, |editor, cx| {
            editor.create_decorations_collection(Vec::new(), cx)
        });
        let subscription = cx.observe_global::<Theme>(|this, cx| this.refresh_syntax(cx));
        Self {
            editor,
            response: None,
            generation: 0,
            preparation: None,
            pretty: true,
            has_payload: false,
            syntax: Vec::new(),
            decorations,
            _theme_subscription: subscription,
        }
    }

    pub(super) fn project(
        &mut self,
        response: Arc<ResponseState>,
        pretty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .response
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, &response))
            && self.pretty == pretty
        {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.preparation = None;
        self.pretty = pretty;
        self.has_payload = source(&response).1;
        self.response = Some(response.clone());

        if !self.has_payload {
            // Empty/loading/error messages are small and do not need a background document.
            self.install(PreparedBody::plain(source(&response).0), window, cx);
            return;
        }

        // Clear the previous document immediately. Never show/copy it under a new response's
        // metadata while preparation is in flight. The Kit entity (and its focus) stays alive.
        self.install(PreparedBody::plain(""), window, cx);
        let generation = self.generation;
        let preparation = cx
            .background_executor()
            .spawn(async move { prepare_body(&response, pretty) });
        self.preparation = Some(cx.spawn_in(window, async move |this, cx| {
            let prepared = preparation.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.complete_preparation(generation, prepared, window, cx);
            });
        }));
    }

    fn complete_preparation(
        &mut self,
        generation: u64,
        prepared: PreparedBody,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Dropping the retained task cancels delivery, but work already running on a worker
        // may finish. Identity also protects against a late result after a new response/mode.
        if generation != self.generation {
            return;
        }
        self.preparation = None;
        self.install(prepared, window, cx);
    }

    fn install(&mut self, prepared: PreparedBody, window: &mut Window, cx: &mut Context<Self>) {
        self.syntax = prepared.syntax;
        self.editor.update(cx, |editor, cx| {
            editor.set_value(prepared.text, window, cx);
            editor.set_line_number(self.pretty && self.has_payload, window, cx);
            editor.set_soft_wrap(!self.pretty, window, cx);
        });
        self.refresh_syntax(cx);
        cx.notify();
    }

    fn refresh_syntax(&self, cx: &mut Context<Self>) {
        self.decorations.set(
            self.syntax
                .iter()
                .map(|(range, color)| {
                    TextDecoration::new(
                        range.clone(),
                        HighlightStyle {
                            color: Some(color.resolve(cx).into()),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            cx,
        );
    }

    pub(super) fn clear_selection(&self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let cursor = editor.cursor();
            editor.set_selected_range(cursor..cursor, cx);
        });
    }
}

struct PreparedBody {
    text: SharedString,
    syntax: Vec<(Range<usize>, ColorToken)>,
}

impl PreparedBody {
    fn plain(text: &str) -> Self {
        Self {
            text: text.to_owned().into(),
            syntax: Vec::new(),
        }
    }
}

// Parsing, formatting, UTF-8 token scanning and the editor's owned text allocation all run
// on a worker. Only installing the finished document/decorations requires the UI thread.
fn prepare_body(response: &ResponseState, pretty: bool) -> PreparedBody {
    let (source, has_payload) = source(response);
    if pretty && has_payload {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(source) {
            let text = serde_json::to_string_pretty(&json).expect("JSON value serializes");
            let syntax = json_syntax(&text);
            return PreparedBody {
                text: text.into(),
                syntax,
            };
        }
    }
    PreparedBody::plain(source)
}

fn source(response: &ResponseState) -> (&str, bool) {
    match response {
        ResponseState::Success {
            is_binary: true, ..
        } => ("", false),
        ResponseState::Success { body, .. } => (body, !body.is_empty()),
        ResponseState::Historical { response, .. } => match &response.body {
            HistoricalResponseBody::Empty => ("Empty response body", false),
            HistoricalResponseBody::Unsupported => ("Body not stored", false),
            HistoricalResponseBody::Text(text) | HistoricalResponseBody::TruncatedText(text) => {
                (text, !text.is_empty())
            }
        },
        ResponseState::HistoricalUnavailable { .. } => {
            ("This older History entry did not store a response.", false)
        }
        ResponseState::Error { message } => (message, false),
        ResponseState::Cancelled => ("Request cancelled by user", false),
        ResponseState::NotSent | ResponseState::Loading => ("", false),
    }
}

impl Focusable for ResponseBody {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

impl Render for ResponseBody {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle(cx);
        let menu_focus = focus.clone();
        div()
            .id("response-content")
            .debug_selector(|| "response-content".into())
            .key_context("ResponseContent")
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(SIDEBAR.resolve(cx))
            // Add to Kit's built-in editor padding to match the prototype's 19px
            // top inset and 48px gutter / 22px Raw inset. The right/bottom edges
            // stay unpadded so Kit's scrollbars remain against the pane edges.
            .pt(rems(11. / 16.))
            .pl(rems(if self.pretty { 14. / 16. } else { 12. / 16. }))
            // Kit's default Copy copies the current line when nothing is selected. This
            // viewer keeps selection-copy separate from the toolbar's full-response copy.
            .capture_action(cx.listener(|this, _: &Copy, _, cx| {
                if this.editor.read(cx).selected_range().is_empty() {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.clear_selection(cx)))
            .child(
                Editor::new(&self.editor)
                    .readonly(true)
                    .appearance(false)
                    .bordered(false)
                    .h(relative(1.))
                    .min_h_0()
                    .font_family(FONT_MONO)
                    .text_size(m::CODE)
                    .text_color(if self.pretty { TEXT } else { CODE_STRING }.resolve(cx))
                    .line_height(relative(m::CODE_LINE_HEIGHT))
                    .bg(SIDEBAR.resolve(cx))
                    .rounded_none()
                    .aria_label("Response body"),
            )
            .when(self.preparation.is_some(), |root| {
                root.child(
                    div()
                        .id("response-preparing")
                        .debug_selector(|| "response-preparing".into())
                        .role(gpui::Role::Status)
                        .absolute()
                        .top_0()
                        .left_0()
                        .px_4()
                        .py_3()
                        .text_size(m::BODY)
                        .text_color(MUTED.resolve(cx))
                        .child("Preparing response…"),
                )
            })
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button == MouseButton::Right {
                    focus.focus(window, cx);
                }
            })
            .context_menu(move |menu, _, _| {
                edit_popup_menu(
                    menu,
                    menu_focus.clone(),
                    vec![
                        ("Copy", Box::new(Copy)),
                        ("Select All", Box::new(SelectAll)),
                    ],
                )
            })
    }
}

// Only applied to serde_json's validated, pretty-printed output. Byte ranges are passed to
// Kit's decoration collection; Kit owns UTF-8 geometry and painting. This small palette
// adapter avoids enabling the pinned Kit release's unavailable tree-sitter dependency.
fn json_syntax(text: &str) -> Vec<(Range<usize>, ColorToken)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut ix = 0;
    while ix < bytes.len() {
        let start = ix;
        let color = match bytes[ix] {
            b'"' => {
                ix += 1;
                while ix < bytes.len() {
                    match bytes[ix] {
                        b'\\' => ix += 2,
                        b'"' => {
                            ix += 1;
                            break;
                        }
                        _ => ix += 1,
                    }
                }
                if bytes.get(ix) == Some(&b':') {
                    CODE_KEY
                } else {
                    CODE_STRING
                }
            }
            b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => {
                while ix < bytes.len()
                    && !bytes[ix].is_ascii_whitespace()
                    && !b",]}".contains(&bytes[ix])
                {
                    ix += 1;
                }
                CODE_NUMBER
            }
            _ => {
                ix += 1;
                continue;
            }
        };
        spans.push((start..ix, color));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::HistoricalResponse;

    #[gpui::test]
    fn late_preparation_cannot_replace_a_new_mode_response_or_cancellation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::ui::kit::init);
        let (handle, body) = cx
            .update(|cx| {
                gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| ResponseBody::new(window, cx))
                })
            })
            .unwrap();
        let original = Arc::new(ResponseState::Success {
            status: 200,
            body: r#"{"message":"中😀","n":1}"#.into(),
            body_bytes: r#"{"message":"中😀","n":1}"#.as_bytes().into(),
            is_binary: false,
            headers: Vec::new(),
            elapsed_ms: 1,
        });
        let late_generation = cx
            .update_window(handle, |_, window, cx| {
                body.update(cx, |body, cx| {
                    body.project(original.clone(), true, window, cx);
                    let generation = body.generation;
                    assert!(body.preparation.is_some());
                    assert!(
                        body.editor.read(cx).value().is_empty(),
                        "formatting must not run inline"
                    );
                    body.project(original.clone(), false, window, cx);
                    // Simulate an already-running worker delivering after it was superseded.
                    body.complete_preparation(
                        generation,
                        prepare_body(&original, true),
                        window,
                        cx,
                    );
                    assert!(body.editor.read(cx).value().is_empty());
                    generation
                })
            })
            .unwrap();
        cx.run_until_parked();
        body.read_with(cx, |body, cx| {
            assert!(body.preparation.is_none());
            assert_eq!(body.editor.read(cx).value().as_ref(), source(&original).0);
            assert!(body.syntax.is_empty());
        });
        cx.update_window(handle, |_, window, cx| {
            body.update(cx, |body, cx| {
                // A same-byte response is a new identity and clears the previous document too.
                body.project(Arc::new((*original).clone()), true, window, cx);
                assert!(body.preparation.is_some());
                body.project(Arc::new(ResponseState::Cancelled), true, window, cx);
                body.complete_preparation(
                    late_generation,
                    prepare_body(&original, true),
                    window,
                    cx,
                );
                assert_eq!(
                    body.editor.read(cx).value().as_ref(),
                    "Request cancelled by user"
                );
            })
        })
        .unwrap();
        cx.run_until_parked();
        body.read_with(cx, |body, cx| {
            assert!(body.preparation.is_none());
            assert_eq!(
                body.editor.read(cx).value().as_ref(),
                "Request cancelled by user"
            );
            assert!(body.syntax.is_empty());
        });
    }

    #[test]
    fn preparation_keeps_raw_invalid_json_and_truncated_history_exact() {
        for raw in [
            "",
            "hello 中😀\r\n",
            r#"{"incomplete":"#,
            r#"{ "a":1, "b":"中😀" }"#,
        ] {
            let response = ResponseState::Success {
                status: 200,
                body: raw.into(),
                body_bytes: raw.as_bytes().into(),
                is_binary: false,
                headers: Vec::new(),
                elapsed_ms: 1,
            };
            assert_eq!(prepare_body(&response, false).text.as_ref(), raw);
            if serde_json::from_str::<serde_json::Value>(raw).is_err() {
                let pretty = prepare_body(&response, true);
                assert_eq!(pretty.text.as_ref(), raw);
                assert!(pretty.syntax.is_empty());
            }
        }
        let response = ResponseState::Historical {
            entry_id: "history".into(),
            response: HistoricalResponse {
                status: 200,
                headers: Vec::new(),
                body: HistoricalResponseBody::TruncatedText(r#"{"partial":"#.into()),
                media_type: Some("application/json".into()),
                elapsed_ms: 1,
                original_size: 100,
                persisted_size: 11,
            },
        };
        assert_eq!(
            prepare_body(&response, true).text.as_ref(),
            r#"{"partial":"#
        );
    }

    #[test]
    fn json_palette_preserves_escaped_unicode_strings_and_scalar_tokens() {
        let value = serde_json::json!({
            "escaped\"中": "hello \\\"😀", "array": [-12.5, true, false, null, 1.2e30],
        });
        let text = serde_json::to_string_pretty(&value).unwrap();
        let spans = json_syntax(&text);
        let tokens = spans
            .iter()
            .map(|(range, color)| (&text[range.clone()], *color))
            .collect::<Vec<_>>();
        for (token, color) in [
            (r#""escaped\"中""#, CODE_KEY),
            (r#""hello \\\"😀""#, CODE_STRING),
            ("-12.5", CODE_NUMBER),
            ("true", CODE_NUMBER),
            ("false", CODE_NUMBER),
            ("null", CODE_NUMBER),
            ("1.2e+30", CODE_NUMBER),
        ] {
            assert!(tokens.contains(&(token, color)), "{token:?}: {tokens:?}");
        }
    }

    #[test]
    fn historical_placeholders_and_errors_are_plain_text_without_line_numbers() {
        for (body, expected, payload) in [
            (HistoricalResponseBody::Empty, "Empty response body", false),
            (
                HistoricalResponseBody::Unsupported,
                "Body not stored",
                false,
            ),
            (HistoricalResponseBody::Text("中😀".into()), "中😀", true),
            (
                HistoricalResponseBody::TruncatedText("partial".into()),
                "partial",
                true,
            ),
        ] {
            let response = ResponseState::Historical {
                entry_id: "history-1".into(),
                response: HistoricalResponse {
                    status: 200,
                    headers: Vec::new(),
                    body,
                    media_type: None,
                    elapsed_ms: 1,
                    original_size: 0,
                    persisted_size: 0,
                },
            };
            assert_eq!(source(&response), (expected, payload));
        }
        assert_eq!(
            source(&ResponseState::Error {
                message: "network failure".into()
            }),
            ("network failure", false)
        );
        assert_eq!(
            source(&ResponseState::Cancelled),
            ("Request cancelled by user", false)
        );
    }
}
