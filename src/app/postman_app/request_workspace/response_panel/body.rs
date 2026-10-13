//! Retained response document. Kit owns layout, selection, highlighting and both scroll axes.
use crate::{
    app::ResponseState,
    models::HistoricalResponseBody,
    ui::{
        components::common::edit_context_menu::edit_popup_menu,
        theme::{
            metrics as m, ColorToken, CODE_KEY, CODE_NUMBER, CODE_STRING, FONT_MONO, SIDEBAR, TEXT,
        },
    },
};
use gpui::{
    div, relative, rems, App, AppContext, Context, Entity, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_kit::component::{
    input::{
        Copy, Editor, EditorState, Escape, SelectAll, TextDecoration, TextDecorationCollection,
    },
    menu::ContextMenuExt,
    Theme,
};
use std::ops::Range;

pub(super) struct ResponseBody {
    pub(super) editor: Entity<EditorState>,
    source: Option<SharedString>,
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
            source: None,
            pretty: true,
            has_payload: false,
            syntax: Vec::new(),
            decorations,
            _theme_subscription: subscription,
        }
    }

    pub(super) fn project(
        &mut self,
        response: &ResponseState,
        pretty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (source, has_payload) = source(response);
        if self.source.as_deref() == Some(source)
            && self.pretty == pretty
            && self.has_payload == has_payload
        {
            return;
        }
        self.source = Some(source.to_owned().into());
        self.pretty = pretty;
        self.has_payload = has_payload;
        let json = (pretty && has_payload)
            .then(|| serde_json::from_str::<serde_json::Value>(source).ok())
            .flatten();
        let value = json.as_ref().map_or_else(
            || source.to_owned(),
            |value| serde_json::to_string_pretty(value).expect("JSON value serializes"),
        );
        self.syntax = if json.is_some() {
            json_syntax(&value)
        } else {
            Vec::new()
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_value(value, window, cx);
            editor.set_line_number(pretty && has_payload, window, cx);
            editor.set_soft_wrap(!pretty, window, cx);
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

fn source(response: &ResponseState) -> (&str, bool) {
    match response {
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
