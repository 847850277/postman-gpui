use crate::{
    app::{ResponseState, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{
            FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT,
            SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, px, rgb, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};

/// Per-request transport policy. The input entity owns only editing state; the configured
/// deadline remains part of the active request draft in `WorkspaceViewModel`.
pub(in crate::app::postman_app::request_workspace) struct OptionsPane {
    view_model: Entity<WorkspaceViewModel>,
    timeout_input: Entity<HeaderInput>,
    _subscriptions: Vec<Subscription>,
}

impl OptionsPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let timeout_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("0")
                .with_embedded_chrome(true)
        });
        let subscriptions = vec![
            cx.subscribe(&timeout_input, Self::on_timeout_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];
        let mut pane = Self {
            view_model,
            timeout_input,
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
    }

    fn on_timeout_event(
        &mut self,
        input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        let HeaderInputEvent::ValueChanged(value) = event else {
            return;
        };
        let parsed = if value.is_empty() {
            Some(0)
        } else if value.chars().all(|character| character.is_ascii_digit()) {
            value.parse::<u64>().ok()
        } else {
            None
        };

        if let Some(timeout_ms) = parsed {
            self.view_model.update(cx, |view_model, cx| {
                view_model.set_timeout_ms(timeout_ms);
                cx.notify();
            });
        } else {
            let timeout_ms = self.view_model.read(cx).timeout_ms();
            input.update(cx, |input, cx| {
                input.project_content(timeout_value(timeout_ms), cx)
            });
        }
        cx.notify();
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let timeout_ms = self.view_model.read(cx).timeout_ms();
        self.timeout_input.update(cx, |input, cx| {
            input.project_content(timeout_value(timeout_ms), cx)
        });
        cx.notify();
    }
}

impl Render for OptionsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (timeout_ms, request_id, in_flight, lifecycle) = {
            let view_model = self.view_model.read(cx);
            let lifecycle = match view_model.response() {
                ResponseState::NotSent => "Not sent",
                ResponseState::Loading => "ResponseState::Loading",
                ResponseState::Cancelled => "ResponseState::Cancelled",
                ResponseState::Success { .. } => "ResponseState::Success",
                ResponseState::Historical { .. } => "ResponseState::Historical",
                ResponseState::HistoricalUnavailable { .. } => {
                    "ResponseState::Historical · unavailable"
                }
                ResponseState::Error { message }
                    if message.starts_with("Request timed out after") =>
                {
                    "ResponseState::Error · timeout"
                }
                ResponseState::Error { .. } => "ResponseState::Error",
            };
            (
                view_model.timeout_ms(),
                view_model.active_request_id(),
                view_model.in_flight_count(),
                lifecycle,
            )
        };
        let timeout_enabled = timeout_ms > 0;
        let timeout_status_selector = if timeout_enabled {
            "request-timeout-enabled"
        } else {
            "request-timeout-disabled"
        };

        div()
            .debug_selector(|| "request-options-panel".into())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "timeout-configuration".into())
                    .h(px(64.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL_ALT))
                    .child(
                        div()
                            .w(px(150.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .font_family(FONT_UI)
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Request timeout"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Per request · 0 disables"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "request-timeout-input".into())
                            .h(px(36.0))
                            .w(px(180.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .px_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(INFO))
                            .bg(rgb(PANEL))
                            .child(self.timeout_input.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "request-timeout-unit".into())
                            .font_family(FONT_MONO)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(11.0))
                            .text_color(rgb(INFO))
                            .child("ms"),
                    )
                    .child(
                        div()
                            .debug_selector(move || timeout_status_selector.into())
                            .h(px(26.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .bg(rgb(if timeout_enabled { OK_SOFT } else { INFO_SOFT }))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(if timeout_enabled { OK } else { MUTED }))
                            .child(if timeout_enabled {
                                format!("{} ms deadline", format_number(timeout_ms))
                            } else {
                                "Deadline disabled".to_string()
                            }),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "request-timeout-contract".into())
                    .px_3()
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        "The deadline is captured when Send starts. Timeout and user cancellation remain distinct terminal states.",
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "request-lifecycle-state".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(INFO_SOFT))
                    .font_family(FONT_MONO)
                    .text_size(px(11.0))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(INFO))
                            .child(lifecycle),
                    )
                    .child(
                        div()
                            .debug_selector(|| "request-id-state".into())
                            .text_color(rgb(TEXT))
                            .child(match request_id {
                                Some(request_id) => format!("request_id={request_id}"),
                                None => "request_id=None".to_string(),
                            }),
                    )
                    .child(
                        div()
                            .debug_selector(|| "request-in-flight-count".into())
                            .text_color(rgb(TEXT))
                            .child(format!("in_flight={in_flight}")),
                    ),
            )
    }
}

fn timeout_value(timeout_ms: u64) -> String {
    if timeout_ms == 0 {
        String::new()
    } else {
        timeout_ms.to_string()
    }
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted
}
