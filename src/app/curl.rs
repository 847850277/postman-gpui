//! Ephemeral cURL dialogs. The existing Kit Root owns dismissal and focus restoration;
//! the workspace owns imported requests, and the converter owns request semantics.
use super::{FocusNextControl, FocusPreviousControl, WorkspaceViewModel};
use crate::{
    models::curl::{self, CurlCode, CurlError, CurlImport},
    ui::theme::{metrics as m, ERROR, FONT_MONO, LINE, MUTED, OK, PANEL_ALT, TEXT},
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        input::{InputEvent, Textarea, TextareaState},
        scroll::ScrollableElement,
        Disableable, Sizable, WindowExt,
    },
};
use gpui_kit::{
    div, prelude::FluentBuilder, rems, App, AppContext, ClipboardItem, Context, Entity, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Subscription, Window,
};
use std::rc::Rc;

type OnImport = Rc<dyn Fn(&mut Window, &mut App)>;
const EXAMPLE: &str = "curl 'https://httpbingo.org/post' \\\n  --header 'Accept: application/json' \\\n  --json '{\"name\":\"Maya Chen\"}'";

pub(super) fn open(
    workspace: Entity<WorkspaceViewModel>,
    import: bool,
    on_import: OnImport,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }
    let code = if import {
        None
    } else {
        workspace
            .read(cx)
            .active_request()
            .map(|r| curl::export(r.request_draft()))
    };
    let editor = cx.new(|cx| CurlDialog::new(workspace, code, on_import, window, cx));
    let content = editor.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
        let state = content.read(cx);
        let width = (window.rem_size() * if state.code.is_none() { 40. } else { 35. })
            .min(window.viewport_size().width - window.rem_size() * 2.);
        let height = state.dialog_height(window);
        let view = content.clone();
        dialog
            .w(width)
            .h(height)
            .margin_top((window.viewport_size().height - height) / 2.)
            .p_0()
            .gap_0()
            .close_button(false)
            .content(move |c, _, _| c.p_0().min_h_0().child(view.clone()))
            // Multiline Enter belongs to the editor; confirmation uses the explicit button.
            .on_ok(|_, _, _| false)
    });
    editor.update(cx, |editor, cx| editor.focus(window, cx));
}

struct CurlDialog {
    workspace: Entity<WorkspaceViewModel>,
    input: Entity<TextareaState>,
    code: Option<Result<CurlCode, CurlError>>,
    preview: Option<Result<CurlImport, CurlError>>,
    copied: bool,
    copy_failed: bool,
    submitted: bool,
    scroll: ScrollHandle,
    on_import: OnImport,
    _subscription: Subscription,
}
impl CurlDialog {
    fn new(
        workspace: Entity<WorkspaceViewModel>,
        code: Option<Result<CurlCode, CurlError>>,
        on_import: OnImport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = code
            .as_ref()
            .and_then(|v| v.as_ref().ok())
            .map(|c| c.text().to_owned())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(value)
                .placeholder("curl 'https://httpbingo.org/get'")
        });
        let subscription = cx.subscribe(&input, |this, input, event, cx| {
            if matches!(event, InputEvent::Change) && this.code.is_none() {
                this.preview = preview(&input.read(cx).value());
                cx.notify();
            }
        });
        Self {
            workspace,
            input,
            code,
            preview: None,
            copied: false,
            copy_failed: false,
            submitted: false,
            scroll: ScrollHandle::new(),
            on_import,
            _subscription: subscription,
        }
    }
    fn input_height(&self, window: &Window) -> gpui_kit::Pixels {
        if self.code.is_none() {
            return (window.rem_size() * 13.).min(window.viewport_size().height * 0.29);
        }
        let rows = self
            .code
            .as_ref()
            .and_then(|c| c.as_ref().ok())
            .map_or(0, |code| {
                code.text()
                    .lines()
                    .take(30)
                    .map(|line| line.chars().count().max(1).div_ceil(68))
                    .sum::<usize>()
            });
        (window.rem_size() * (rows.clamp(3, 18) as f32 * 1.3 + 1.5))
            .min(window.viewport_size().height * 0.50)
    }
    fn dialog_height(&self, window: &Window) -> gpui_kit::Pixels {
        let height = if self.code.is_none() {
            window.rem_size()
                * match self.preview {
                    Some(Ok(_)) => 37.,
                    Some(Err(_)) => 34.,
                    None => 30.,
                }
        } else if matches!(self.code, Some(Err(_))) {
            window.rem_size() * 17.
        } else {
            self.input_height(window) + window.rem_size() * 13.
        };
        height.min(window.viewport_size().height - window.rem_size() * 2.)
    }
    fn focus(&self, window: &mut Window, cx: &mut App) {
        // Invalid exports render feedback without an editor. Keep Root's dialog focus
        // in that case so Escape and Tab still reach visible controls.
        if !matches!(self.code, Some(Err(_))) {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitted || self.code.is_some() {
            return;
        }
        // Read the retained editor at activation time, not a possibly stale rendered preview.
        self.preview = preview(&self.input.read(cx).value());
        if let Some(Ok(parsed)) = &self.preview {
            let draft = parsed.draft().clone();
            self.submitted = true;
            self.workspace.update(cx, |workspace, cx| {
                workspace.import_request(draft);
                cx.notify();
            });
            window.close_dialog(cx);
            (self.on_import)(window, cx);
        } else {
            self.focus(window, cx);
            cx.notify();
        }
    }
    fn switch_to_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.code = None;
        self.preview = None;
        self.copied = false;
        self.copy_failed = false;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.focus(window, cx);
        cx.notify();
    }
    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(Ok(code)) = &self.code {
            let text = code.text().to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            self.copied =
                cx.read_from_clipboard().and_then(|v| v.text()).as_deref() == Some(text.as_str());
            self.copy_failed = !self.copied;
            cx.notify();
        }
    }
    fn feedback(&self, cx: &App) -> gpui_kit::AnyElement {
        if let Some(result) = &self.code {
            return match result {
                Err(error) => error_text(error).text_color(ERROR.resolve(cx)).into_any_element(),
                Ok(code) => div().text_size(m::LABEL).text_color(MUTED.resolve(cx))
                    .flex().flex_col().gap_2()
                    .child(if code.references_files() { "References local files. cURL reads them when you run it; file-based commands cannot be imported here." } else { "Uses the current request configuration." })
                    .when(code.requires_modern_curl(), |v| v.child("Requires cURL 8.16+ to preserve redirect behavior with this method."))
                    .when(self.copy_failed, |v| v.child(div().text_color(ERROR.resolve(cx)).child("Clipboard copy could not be confirmed. Select the command and copy it manually.")))
                    .into_any_element(),
            };
        }
        match &self.preview {
            Some(Err(error)) => error_text(error)
                .text_color(ERROR.resolve(cx))
                .into_any_element(),
            Some(Ok(parsed)) => {
                let c = parsed.draft().construct();
                let r = c.request();
                let auth = r
                    .headers
                    .iter()
                    .filter(|(n, _)| n.eq_ignore_ascii_case("authorization"))
                    .collect::<Vec<_>>();
                let auth_label = match auth.as_slice() {
                    [] => "No auth",
                    [(_, v)] if v.starts_with("Bearer ") => "Bearer token",
                    [(_, v)] if v.starts_with("Basic ") => "Basic auth",
                    _ => "Authorization headers",
                };
                let body = if r.body.is_none() {
                    "No body".into()
                } else {
                    let kind = match parsed.draft().body_kind() {
                        crate::models::BodyKind::Json => "JSON body",
                        crate::models::BodyKind::UrlEncoded => "URL encoded body",
                        _ => "Raw body",
                    };
                    format!("{kind} · {} B", r.body.as_text().map_or(0, str::len))
                };
                let options = c.request_options();
                let redirects = if options.redirect_policy == crate::models::RedirectPolicy::Follow
                {
                    format!("Follow redirects · max {}", options.max_redirect_hops)
                } else {
                    "Do not follow redirects".into()
                };
                let timeout = options
                    .timeout_ms
                    .map(|ms| format!("Timeout {ms} ms"))
                    .unwrap_or_else(|| "No timeout".into());
                div()
                    .debug_selector(|| "curl-preview".into())
                    .p_3()
                    .rounded(m::RADIUS)
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .bg(PANEL_ALT.resolve(cx))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_color(OK.resolve(cx))
                            .font_weight(m::MEDIUM)
                            .child("Ready to import"),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .child(format!("{} {}", r.method, r.url)),
                    )
                    .child(div().text_color(MUTED.resolve(cx)).child(format!(
                        "{} headers  ·  {body}  ·  {auth_label}",
                        r.headers.len()
                    )))
                    .child(
                        div()
                            .text_color(MUTED.resolve(cx))
                            .child(format!("{redirects}  ·  {timeout}")),
                    )
                    .into_any_element()
            }
            None => div().into_any_element(),
        }
    }
}
fn preview(text: &str) -> Option<Result<CurlImport, CurlError>> {
    (!text.trim().is_empty()).then(|| curl::parse(text))
}
fn error_text(error: &CurlError) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id("curl-error")
        .debug_selector(|| "curl-error".into())
        .text_size(m::LABEL)
        .child(error.to_string())
}
impl Render for CurlDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let import = self.code.is_none();
        let valid = matches!(self.preview, Some(Ok(_)));
        let code_valid = matches!(self.code, Some(Ok(_)));
        let input_height = self.input_height(window);
        let header = div()
            .flex_none()
            .h(rems(4.))
            .px_5()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                gpui_kit::component::Icon::new(if import {
                    IconName::Download
                } else {
                    IconName::Code
                })
                .size(m::ICON),
            )
            .child(
                div()
                    .font_weight(m::SEMIBOLD)
                    .text_size(m::BODY)
                    .child(if import {
                        "Import cURL"
                    } else {
                        "Request code"
                    }),
            )
            .child(div().flex_1())
            .when(!import, |header| {
                header.child(div().text_color(MUTED.resolve(cx)).child("cURL"))
            })
            .child(
                Button::new("curl-close")
                    .debug_selector(|| "curl-close".into())
                    .ghost()
                    .small()
                    .icon(IconName::X)
                    .accessibility_label("Close cURL dialog")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            );
        let body = div()
            .id("curl-content")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .when(import, |body| {
                body.child(
                    div()
                        .text_color(MUTED.resolve(cx))
                        .child("Paste a cURL command to create an HTTP request."),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child("cURL command")
                        .child(div().flex_1())
                        .child(
                            Button::new("curl-example")
                                .debug_selector(|| "curl-example".into())
                                .ghost()
                                .xsmall()
                                .label("Try example")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.input.update(cx, |input, cx| {
                                        input.set_value(EXAMPLE, window, cx)
                                    });
                                    this.preview = preview(EXAMPLE);
                                    this.focus(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(import || code_valid, |body| {
                body.child(
                    div()
                        .key_context("CurlCommandInput")
                        // A pasted-command field uses Tab for form navigation, not indentation.
                        // Kit owns the focus order and the surrounding dialog trap.
                        .on_action(|_: &FocusNextControl, window, cx| window.focus_next(cx))
                        .on_action(|_: &FocusPreviousControl, window, cx| window.focus_prev(cx))
                        .debug_selector(move || {
                            if import { "curl-input" } else { "curl-code" }.into()
                        })
                        .min_w_0()
                        .flex_none()
                        .child(
                            Textarea::new(&self.input)
                                .accessibility_id("curl-text")
                                .aria_label(if import {
                                    "cURL command"
                                } else {
                                    "Generated cURL command"
                                })
                                .readonly(!import)
                                .bordered(import)
                                .appearance(import)
                                .h(input_height)
                                .font_family(FONT_MONO)
                                .text_size(rems(0.75))
                                .text_color(OK.resolve(cx))
                                .when(import, |input| input.bg(PANEL_ALT.resolve(cx))),
                        ),
                )
            })
            .when(import, |body| {
                body.child(div().text_color(MUTED.resolve(cx)).child(
                    "POSIX shell syntax. Paste literal values; commands are never executed.",
                ))
            })
            .child(self.feedback(cx))
            .vertical_scrollbar(&self.scroll);
        let footer = div()
            .flex_none()
            .min_h(rems(3.5))
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(LINE.resolve(cx))
            .flex()
            .items_center()
            .gap_2()
            .when(import, |footer| {
                footer
                    .child(
                        div()
                            .text_color(MUTED.resolve(cx))
                            .child("Opens in a new request tab"),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("curl-cancel")
                            .debug_selector(|| "curl-cancel".into())
                            .ghost()
                            .small()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("curl-submit")
                            .debug_selector(|| "curl-submit".into())
                            .primary()
                            .small()
                            .label("Import request")
                            .disabled(!valid || self.submitted)
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    )
            })
            .when(!import, |footer| {
                footer
                    .child(
                        Button::new("curl-code-import")
                            .debug_selector(|| "curl-code-import".into())
                            .ghost()
                            .small()
                            .icon(IconName::Download)
                            .label("Import cURL")
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.switch_to_import(window, cx)
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("curl-copy")
                            .debug_selector(|| "curl-copy".into())
                            .outline()
                            .small()
                            .icon(if self.copied {
                                IconName::Check
                            } else {
                                IconName::Copy
                            })
                            .label(if self.copied { "Copied" } else { "Copy code" })
                            .disabled(!code_valid)
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
            });
        div()
            .debug_selector(move || {
                if import {
                    "curl-import-dialog"
                } else {
                    "curl-code-dialog"
                }
                .into()
            })
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .text_color(TEXT.resolve(cx))
            .text_size(m::LABEL)
            .child(header)
            .child(body)
            .child(footer)
    }
}
