use gpui::{
    actions, div, fill, point, prelude::FluentBuilder, px, rgb, rgba, App, Bounds, ClipboardItem,
    Context, CursorStyle, Element, ElementId, Entity, FocusHandle, Focusable, FontWeight,
    GlobalElementId, InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render,
    Role, ShapedLine, StatefulInteractiveElement, Style, Styled, Subscription, TextAlign, TextRun,
    Window,
};
use std::{ops::Range, time::Duration};

use crate::{
    app::{ResponseState, WorkspaceViewModel},
    ui::components::common::edit_context_menu::{
        edit_context_menu, EditContextAction, READ_ONLY_ACTIONS,
    },
    ui::theme::{
        CODE_BG, CODE_TEXT, ERROR, FONT_HEADING, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED,
        OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
    },
    utils::formatter::format_response_body,
};

const COPIED_FEEDBACK_DURATION: Duration = Duration::from_secs(2);

actions!(response_viewer, [Copy, SelectAll, CopyResponseBody]);

pub fn setup_response_viewer_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("ctrl-a", SelectAll, None),
        KeyBinding::new("enter", CopyResponseBody, Some("ResponseCopyButton")),
        KeyBinding::new("space", CopyResponseBody, Some("ResponseCopyButton")),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResponsePane {
    Body,
    Headers,
}

/// Response surface owned by the request workspace.
pub struct ResponseViewer {
    view_model: Entity<WorkspaceViewModel>,
    pane: ResponsePane,
    focus_handle: FocusHandle,
    copy_focus_handle: FocusHandle,
    copied_feedback: bool,
    copy_generation: u64,
    selected_range: Range<usize>,
    selection_reversed: bool,
    is_selecting: bool,
    last_bounds: Option<Bounds<Pixels>>,
    last_lines_layout: Vec<(ShapedLine, usize)>, // (shaped_line, char_offset)
    context_menu_position: Option<Point<Pixels>>,
    _view_model_subscription: Subscription,
}

impl Focusable for ResponseViewer {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ResponseViewer {
    pub fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let view_model_subscription = cx.observe(&view_model, |this, _, cx| {
            this.copied_feedback = false;
            this.copy_generation = this.copy_generation.wrapping_add(1);
            cx.notify();
        });
        Self {
            view_model,
            pane: ResponsePane::Body,
            focus_handle: cx.focus_handle(),
            copy_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            copied_feedback: false,
            copy_generation: 0,
            selected_range: 0..0,
            selection_reversed: false,
            is_selecting: false,
            last_bounds: None,
            last_lines_layout: Vec::new(),
            context_menu_position: None,
            _view_model_subscription: view_model_subscription,
        }
    }

    fn raw_response_body(&self, cx: &App) -> Option<String> {
        match self.view_model.read(cx).response() {
            ResponseState::Success { body, .. } if !body.is_empty() => Some(body.clone()),
            _ => None,
        }
    }

    fn copy_raw_response_body(&mut self, cx: &mut Context<Self>) {
        let Some(body) = self.raw_response_body(cx) else {
            return;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(body));
        self.copy_generation = self.copy_generation.wrapping_add(1);
        let copy_generation = self.copy_generation;
        self.copied_feedback = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(COPIED_FEEDBACK_DURATION)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == copy_generation {
                    this.copied_feedback = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn copy_response_body(
        &mut self,
        _: &CopyResponseBody,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_raw_response_body(cx);
    }

    fn click_copy_response_body(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.copy_focus_handle.focus(window, cx);
        self.copy_raw_response_body(cx);
    }

    fn get_content(&self, cx: &App) -> String {
        let view_model = self.view_model.read(cx);
        match (view_model.response(), self.pane) {
            (ResponseState::Success { body, .. }, ResponsePane::Body) => format_response_body(body),
            (ResponseState::Success { headers, .. }, ResponsePane::Headers) => headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            (ResponseState::Error { message }, _) => message.clone(),
            (ResponseState::Cancelled, _) => "Request cancelled by user".to_string(),
            _ => String::new(),
        }
    }

    fn pane_tab(
        &self,
        pane: ResponsePane,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.pane == pane;
        div()
            .h_full()
            .flex()
            .items_center()
            .px_2()
            .cursor_pointer()
            .when(active, |d| {
                d.text_color(rgb(TEXT)).font_weight(FontWeight::SEMIBOLD)
            })
            .when(!active, |d| {
                d.text_color(rgb(MUTED))
                    .hover(|s| s.text_color(rgb(SUBTEXT)))
            })
            .text_size(px(12.0))
            .font_family(FONT_UI)
            .child(label)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.pane = pane;
                    this.selected_range = 0..0;
                    cx.notify();
                }),
            )
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let content = self.get_content(cx);
            if !content.is_empty() {
                let selected_text: String = content
                    .chars()
                    .skip(self.selected_range.start)
                    .take(
                        self.selected_range
                            .end
                            .saturating_sub(self.selected_range.start),
                    )
                    .collect();

                if !selected_text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
                }
            }
        }
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.get_content(cx);
        self.selected_range = 0..content.chars().count();
        cx.notify();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu_position = None;
        self.is_selecting = true;
        if event.modifiers.shift {
            self.response_select_to(self.index_for_mouse_position(event.position, cx), cx);
        } else {
            self.response_move_to(self.index_for_mouse_position(event.position, cx), cx);
        }
    }

    fn response_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let offset = self.index_for_mouse_position(event.position, cx);
            self.response_select_to(offset, cx);
        }
    }

    fn open_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.is_selecting = false;
        self.context_menu_position = Some(event.position);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn handle_context_menu_action(
        &mut self,
        action: EditContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            EditContextAction::Copy => self.copy(&Copy, window, cx),
            EditContextAction::SelectAll => self.select_all(&SelectAll, window, cx),
            EditContextAction::Cut | EditContextAction::Paste | EditContextAction::Dismiss => {}
        }
        self.context_menu_position = None;
        cx.notify();
    }

    fn response_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>, cx: &App) -> usize {
        let content = self.get_content(cx);
        if content.is_empty() {
            return 0;
        }

        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return content.chars().count();
        }

        if self.last_lines_layout.is_empty() {
            return 0;
        }

        let line_height = bounds.size.height / self.last_lines_layout.len() as f32;
        let mut line_index = ((position.y - bounds.top()) / line_height).floor() as usize;
        line_index = line_index.min(self.last_lines_layout.len().saturating_sub(1));

        let (shaped_line, line_char_offset) = &self.last_lines_layout[line_index];
        let x_in_line = position.x - bounds.left();
        let offset_in_line = shaped_line.closest_index_for_x(x_in_line);

        let absolute_offset = line_char_offset.saturating_add(offset_in_line);
        absolute_offset.min(content.chars().count())
    }

    fn render_selectable_content(
        &self,
        _content: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("response-content")
            .debug_selector(|| "response-content".into())
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
            .cursor_text()
            .w_full()
            .h_full()
            .min_h_0()
            .p_3()
            .bg(rgb(CODE_BG))
            .text_color(rgb(CODE_TEXT))
            .font_family(FONT_MONO)
            .text_size(px(13.0))
            .overflow_scroll()
            .child(MultiLineTextElement {
                viewer: cx.entity().clone(),
            })
    }
}

// Custom text element for rendering multi-line response content with selection
struct MultiLineTextElement {
    viewer: Entity<ResponseViewer>,
}

struct PrepaintState {
    lines: Vec<(ShapedLine, usize)>,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for MultiLineTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MultiLineTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();

        let viewer = self.viewer.read(cx);
        let content = viewer.get_content(cx);
        let line_count = content.lines().count().max(1);
        let line_height = window.line_height();
        style.size.height = (line_height * line_count as f32).into();

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let viewer = self.viewer.read(cx);
        let content = viewer.get_content(cx);
        let selected_range = viewer.selected_range.clone();

        let style = window.text_style();
        let font_size = px(13.0);
        let line_height = window.line_height();

        let lines: Vec<&str> = content.lines().collect();
        let mut shaped_lines = Vec::new();
        let mut char_offset = 0;

        for line in &lines {
            let run = TextRun {
                len: line.len(),
                font: style.font(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            let shaped_line = window.text_system().shape_line(
                (*line).to_string().into(),
                font_size,
                &[run],
                None,
            );

            shaped_lines.push((shaped_line, char_offset));
            char_offset += line.chars().count() + 1;
        }

        let mut selections = Vec::new();
        let mut cursor = None;

        if selected_range.is_empty() && !content.is_empty() {
            let cursor_char = selected_range.start;
            let mut current_offset = 0;

            for (line_idx, (_shaped_line, _)) in shaped_lines.iter().enumerate() {
                let line_len = if line_idx < lines.len() {
                    lines[line_idx].chars().count()
                } else {
                    0
                };

                if cursor_char >= current_offset && cursor_char <= current_offset + line_len {
                    let local_pos = cursor_char - current_offset;
                    let x_pos = if local_pos == 0 {
                        px(0.0)
                    } else {
                        let line_text: String = lines[line_idx].chars().take(local_pos).collect();
                        let temp_run = TextRun {
                            len: line_text.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            line_text.into(),
                            font_size,
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    cursor = Some(fill(
                        Bounds::new(
                            point(
                                bounds.left() + x_pos,
                                bounds.top() + line_height * line_idx as f32,
                            ),
                            gpui::size(px(2.), line_height),
                        ),
                        rgb(INFO),
                    ));
                    break;
                }

                current_offset += line_len + 1;
            }
        } else if !selected_range.is_empty() && !content.is_empty() {
            let mut current_offset = 0;

            for (line_idx, (shaped_line, _)) in shaped_lines.iter().enumerate() {
                let line_len = if line_idx < lines.len() {
                    lines[line_idx].chars().count()
                } else {
                    0
                };

                let line_start = current_offset;
                let line_end = current_offset + line_len;

                if selected_range.end > line_start && selected_range.start < line_end {
                    let sel_start = selected_range.start.max(line_start).min(line_end);
                    let sel_end = selected_range.end.max(line_start).min(line_end);

                    let local_start = sel_start - line_start;
                    let local_end = sel_end - line_start;

                    let start_x = if local_start == 0 {
                        px(0.0)
                    } else {
                        let text_before: String =
                            lines[line_idx].chars().take(local_start).collect();
                        let temp_run = TextRun {
                            len: text_before.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            text_before.into(),
                            font_size,
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    let end_x = if local_end == 0 {
                        px(0.0)
                    } else if local_end >= line_len {
                        shaped_line.width
                    } else {
                        let text_before: String = lines[line_idx].chars().take(local_end).collect();
                        let temp_run = TextRun {
                            len: text_before.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            text_before.into(),
                            font_size,
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    selections.push(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + start_x,
                                bounds.top() + line_height * line_idx as f32,
                            ),
                            point(
                                bounds.left() + end_x,
                                bounds.top() + line_height * (line_idx + 1) as f32,
                            ),
                        ),
                        rgba(0x3366_ff55),
                    ));
                }

                current_offset += line_len + 1;
            }
        }

        self.viewer.update(cx, |viewer, _cx| {
            viewer.last_lines_layout = shaped_lines.clone();
            viewer.last_bounds = Some(bounds);
        });

        PrepaintState {
            lines: shaped_lines,
            selections,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = window.line_height();

        for selection in &prepaint.selections {
            window.paint_quad(selection.clone());
        }

        if let Some(cursor) = &prepaint.cursor {
            window.paint_quad(cursor.clone());
        }

        for (line_idx, (shaped_line, _)) in prepaint.lines.iter().enumerate() {
            let origin = point(
                bounds.origin.x,
                bounds.origin.y + line_height * line_idx as f32,
            );
            shaped_line
                .paint(origin, line_height, TextAlign::Left, None, window, cx)
                .ok();
        }
    }
}

impl Render for ResponseViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (state, is_httpbingo) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.response().clone(),
                view_model.effective_url().contains("httpbingo.org"),
            )
        };
        let pane = self.pane;
        let context_menu_position = self.context_menu_position;
        let body_tab = self.pane_tab(ResponsePane::Body, "Body", cx);
        let headers_tab = self.pane_tab(ResponsePane::Headers, "Headers", cx);
        let has_completed_response = matches!(&state, ResponseState::Success { .. });
        let has_copyable_body =
            matches!(&state, ResponseState::Success { body, .. } if !body.is_empty());
        let copied_feedback = has_copyable_body && self.copied_feedback;
        let copy_is_focused = self.copy_focus_handle.is_focused(window);
        let completed_status = match &state {
            ResponseState::Success { status, .. } => Some(*status),
            _ => None,
        };
        let is_transport_failure = matches!(&state, ResponseState::Error { .. });
        let is_timeout = matches!(
            &state,
            ResponseState::Error { message } if message.starts_with("Request timed out after")
        );
        let is_cancelled = matches!(&state, ResponseState::Cancelled);

        let (status, elapsed, size, status_color) = match &state {
            ResponseState::Success {
                status,
                body,
                elapsed_ms,
                ..
            } => (
                status_label(*status),
                format!("{elapsed_ms} ms"),
                format_bytes(body.len()),
                if *status < 400 { OK } else { ERROR },
            ),
            ResponseState::Loading => ("Sending…".to_string(), String::new(), String::new(), MUTED),
            ResponseState::Cancelled => {
                ("Cancelled".to_string(), String::new(), String::new(), MUTED)
            }
            ResponseState::Error { .. } if is_timeout => {
                ("Timed out".to_string(), String::new(), String::new(), ERROR)
            }
            ResponseState::Error { .. } => (
                "Request failed".to_string(),
                String::new(),
                String::new(),
                ERROR,
            ),
            ResponseState::NotSent => ("Not sent".to_string(), String::new(), String::new(), MUTED),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(14.0))
            .when(context_menu_position.is_none(), |root| {
                root.overflow_hidden()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h_12()
                    .px_4()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h_full()
                            .gap_3()
                            .child(
                                div()
                                    .child("Response")
                                    .text_size(px(16.0))
                                    .font_family(FONT_HEADING)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(TEXT)),
                            )
                            .when(matches!(&state, ResponseState::Success { .. }), |row| {
                                row.child(div().flex().h_full().child(body_tab).child(headers_tab))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .when(has_copyable_body, |row| {
                                row.child(
                                    div()
                                        .id("response-copy-button")
                                        .debug_selector(|| "response-copy-button".into())
                                        .track_focus(&self.copy_focus_handle)
                                        .key_context("ResponseCopyButton")
                                        .role(Role::Button)
                                        .aria_label("Copy full response body")
                                        .h(px(30.0))
                                        .min_w(px(72.0))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap_1()
                                        .rounded(px(7.0))
                                        .border_1()
                                        .border_color(rgb(LINE))
                                        .bg(rgb(PANEL))
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .when(!copied_feedback, |button| {
                                            button.hover(|style| {
                                                style
                                                    .bg(rgb(INFO_SOFT))
                                                    .border_color(rgb(INFO))
                                                    .text_color(rgb(INFO))
                                            })
                                        })
                                        .when(copied_feedback, |button| {
                                            button
                                                .bg(rgb(OK_SOFT))
                                                .border_color(rgb(OK))
                                                .text_color(rgb(OK))
                                        })
                                        .when(copy_is_focused, |button| {
                                            button.border_color(rgb(INFO))
                                        })
                                        .on_action(cx.listener(Self::copy_response_body))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(Self::click_copy_response_body),
                                        )
                                        .child(if copied_feedback { "✓" } else { "⧉" })
                                        .child(
                                            div()
                                                .when(copied_feedback, |label| {
                                                    label.debug_selector(|| {
                                                        "response-copy-feedback".into()
                                                    })
                                                })
                                                .child(if copied_feedback {
                                                    "Copied"
                                                } else {
                                                    "Copy"
                                                }),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .debug_selector(|| "response-status".into())
                                    .text_color(rgb(status_color))
                                    .font_weight(FontWeight::BOLD)
                                    .child(
                                        div()
                                            .when_some(completed_status, |label, status| {
                                                label.debug_selector(move || {
                                                    format!("response-status-{status}")
                                                })
                                            })
                                            .when(is_transport_failure, |label| {
                                                label.debug_selector(|| {
                                                    "response-transport-error".into()
                                                })
                                            })
                                            .when(is_timeout, |label| {
                                                label.debug_selector(|| {
                                                    "response-timeout-error".into()
                                                })
                                            })
                                            .when(is_cancelled, |label| {
                                                label.debug_selector(|| "response-cancelled".into())
                                            })
                                            .child(status),
                                    ),
                            )
                            .when(!elapsed.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(elapsed),
                                )
                            })
                            .when(!size.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(size),
                                )
                            }),
                    ),
            )
            .when(has_completed_response, |root| {
                root.child(
                    div()
                        .debug_selector(|| "response-echo-bar".into())
                        .h(px(36.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .bg(rgb(PANEL_ALT))
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .font_family(FONT_UI)
                        .text_size(px(11.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(OK))
                                .child("●")
                                .child(if is_httpbingo {
                                    "HTTPBingo echo"
                                } else {
                                    "Response payload"
                                }),
                        )
                        .when(is_httpbingo, |bar| {
                            bar.child(div().text_color(rgb(SUBTEXT)).child("stable subset"))
                        }),
                )
            })
            .child(match state {
                ResponseState::NotSent => div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .justify_start()
                    .gap_2()
                    .p_5()
                    .bg(rgb(PANEL_ALT))
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("Send a request to view response"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(SUBTEXT))
                            .child("Status, headers, and payload will appear here."),
                    ),
                ResponseState::Loading => div()
                    .debug_selector(|| "response-loading".into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(CODE_BG))
                    .font_family(FONT_MONO)
                    .text_size(px(13.0))
                    .text_color(rgb(CODE_TEXT))
                    .child("Waiting for the server…"),
                ResponseState::Cancelled => div()
                    .debug_selector(|| "response-cancelled-content".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.render_selectable_content("Request cancelled by user", cx)),
                ResponseState::Success { body, headers, .. } => {
                    let header_text = if headers.is_empty() {
                        "No response headers".to_string()
                    } else {
                        headers
                            .iter()
                            .map(|(k, v)| format!("{k}: {v}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let content = match pane {
                        ResponsePane::Body => body.clone(),
                        ResponsePane::Headers => header_text,
                    };
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_selectable_content(&content, cx))
                }
                ResponseState::Error { message } => div()
                    .when(is_timeout, |content| {
                        content.debug_selector(|| "response-timeout-content".into())
                    })
                    .flex_1()
                    .min_h_0()
                    .child(self.render_selectable_content(&message, cx)),
            })
            .when_some(context_menu_position, |root, position| {
                root.child(edit_context_menu(
                    position,
                    "response-edit-menu",
                    READ_ONLY_ACTIONS,
                    Self::handle_context_menu_action,
                    window,
                    cx,
                ))
            })
    }
}

fn status_label(status: u16) -> String {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Server Error",
        502 => "Bad Gateway",
        503 => "Unavailable",
        _ => "Response",
    };
    format!("{status} {reason}")
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::status_label;

    #[test]
    fn unknown_success_reason_keeps_the_exact_http_status_visible() {
        assert_eq!(status_label(418), "418 Response");
    }
}
