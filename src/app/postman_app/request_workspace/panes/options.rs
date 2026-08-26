use crate::{
    app::{ActivateControl, ResponseState, WorkspaceViewModel},
    models::{RedirectPolicy, MAX_REDIRECT_HOPS},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{
            ACCENT, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT,
            PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, FocusHandle,
    FontWeight, InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement, Render,
    Role, StatefulInteractiveElement, Styled, Subscription, Window,
};

actions!(
    redirect_policy,
    [NextRedirectPolicy, PreviousRedirectPolicy]
);

fn setup_redirect_policy_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("right", NextRedirectPolicy, Some("RedirectPolicy")),
        KeyBinding::new("down", NextRedirectPolicy, Some("RedirectPolicy")),
        KeyBinding::new("left", PreviousRedirectPolicy, Some("RedirectPolicy")),
        KeyBinding::new("up", PreviousRedirectPolicy, Some("RedirectPolicy")),
    ]
}

/// Per-request transport policy. The input entity owns only editing state; the configured
/// deadline remains part of the active request draft in `WorkspaceViewModel`.
pub(in crate::app::postman_app::request_workspace) struct OptionsPane {
    view_model: Entity<WorkspaceViewModel>,
    timeout_input: Entity<HeaderInput>,
    max_redirects_input: Entity<HeaderInput>,
    redirect_policy_focus_handles: Vec<FocusHandle>,
    redirect_stepper_focus_handles: Vec<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

impl OptionsPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys(setup_redirect_policy_key_bindings());
        let timeout_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("0")
                .with_embedded_chrome(true)
        });
        let max_redirects_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("10")
                .with_embedded_chrome(true)
        });
        let subscriptions = vec![
            cx.subscribe(&timeout_input, Self::on_timeout_event),
            cx.subscribe(&max_redirects_input, Self::on_max_redirects_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];
        let mut pane = Self {
            view_model,
            timeout_input,
            max_redirects_input,
            redirect_policy_focus_handles: (0..2)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            redirect_stepper_focus_handles: (0..2)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
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

    fn on_max_redirects_event(
        &mut self,
        input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        let HeaderInputEvent::ValueChanged(value) = event else {
            return;
        };
        if value.is_empty() {
            return;
        }
        let parsed = value
            .chars()
            .all(|character| character.is_ascii_digit())
            .then(|| value.parse::<u32>().ok())
            .flatten()
            .filter(|value| (1..=MAX_REDIRECT_HOPS).contains(value));

        if let Some(max_redirect_hops) = parsed {
            self.view_model.update(cx, |view_model, cx| {
                view_model.set_max_redirect_hops(max_redirect_hops);
                cx.notify();
            });
        } else {
            let max_redirect_hops = self.view_model.read(cx).max_redirect_hops();
            input.update(cx, |input, cx| {
                input.project_content(max_redirect_hops.to_string(), cx)
            });
        }
        cx.notify();
    }

    fn set_redirect_policy(&mut self, policy: RedirectPolicy, cx: &mut Context<Self>) {
        self.view_model.update(cx, |view_model, cx| {
            view_model.set_redirect_policy(policy);
            cx.notify();
        });
        cx.notify();
    }

    fn adjust_max_redirects(&mut self, delta: i32, cx: &mut Context<Self>) {
        let (policy, current) = {
            let view_model = self.view_model.read(cx);
            (view_model.redirect_policy(), view_model.max_redirect_hops())
        };
        if policy != RedirectPolicy::Follow {
            return;
        }
        let next = (current as i32 + delta).clamp(1, MAX_REDIRECT_HOPS as i32) as u32;
        self.view_model.update(cx, |view_model, cx| {
            view_model.set_max_redirect_hops(next);
            cx.notify();
        });
        self.max_redirects_input
            .update(cx, |input, cx| input.project_content(next.to_string(), cx));
        cx.notify();
    }

    fn select_relative_redirect_policy(
        &mut self,
        policy: RedirectPolicy,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = usize::from(policy == RedirectPolicy::DoNotFollow);
        let next = (index as isize + delta).rem_euclid(2) as usize;
        let next_policy = if next == 0 {
            RedirectPolicy::Follow
        } else {
            RedirectPolicy::DoNotFollow
        };
        self.redirect_policy_focus_handles[next].focus(window, cx);
        self.set_redirect_policy(next_policy, cx);
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (timeout_ms, max_redirect_hops) = {
            let view_model = self.view_model.read(cx);
            (view_model.timeout_ms(), view_model.max_redirect_hops())
        };
        self.timeout_input.update(cx, |input, cx| {
            input.project_content(timeout_value(timeout_ms), cx)
        });
        self.max_redirects_input.update(cx, |input, cx| {
            input.project_content(max_redirect_hops.to_string(), cx)
        });
        cx.notify();
    }

    fn render_redirect_policy_button(
        &self,
        policy: RedirectPolicy,
        label: &'static str,
        selector: &'static str,
        selected: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let index = usize::from(policy == RedirectPolicy::DoNotFollow);
        let focus_handle = self.redirect_policy_focus_handles[index].clone();
        let mouse_focus_handle = focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("KeyboardButton RedirectPolicy")
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
            .h(px(30.0))
            .px_3()
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(if selected { ACCENT } else { LINE }))
            .bg(rgb(if selected { ACCENT_SOFT } else { PANEL }))
            .font_family(FONT_UI)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(11.0))
            .text_color(rgb(if selected { ACCENT } else { MUTED }))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)))
            .when(focused, |button| button.border_2().border_color(rgb(INFO)))
            .child(label)
            .on_action(cx.listener(move |this, _: &ActivateControl, _, cx| {
                this.set_redirect_policy(policy, cx)
            }))
            .on_action(
                cx.listener(move |this, _: &NextRedirectPolicy, window, cx| {
                    this.select_relative_redirect_policy(policy, 1, window, cx)
                }),
            )
            .on_action(
                cx.listener(move |this, _: &PreviousRedirectPolicy, window, cx| {
                    this.select_relative_redirect_policy(policy, -1, window, cx)
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    mouse_focus_handle.focus(window, cx);
                    this.set_redirect_policy(policy, cx);
                }),
            )
    }

    fn render_redirect_stepper(
        &self,
        delta: i32,
        label: &'static str,
        selector: &'static str,
        enabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let index = usize::from(delta > 0);
        let focus_handle = self.redirect_stepper_focus_handles[index].clone();
        let mouse_focus_handle = focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("KeyboardButton")
            .role(Role::Button)
            .aria_label(label)
            .h(px(30.0))
            .w(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .font_family(FONT_MONO)
            .font_weight(FontWeight::BOLD)
            .text_size(px(14.0))
            .text_color(rgb(if enabled { TEXT } else { MUTED }))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)))
            })
            .when(focused, |button| button.border_2().border_color(rgb(INFO)))
            .child(if delta < 0 { "−" } else { "+" })
            .on_action(cx.listener(move |this, _: &ActivateControl, _, cx| {
                this.adjust_max_redirects(delta, cx)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    mouse_focus_handle.focus(window, cx);
                    this.adjust_max_redirects(delta, cx);
                }),
            )
    }
}

impl Render for OptionsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            timeout_ms,
            redirect_policy,
            max_redirect_hops,
            effective_request,
            request_id,
            in_flight,
            lifecycle,
        ) = {
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
                view_model.redirect_policy(),
                view_model.max_redirect_hops(),
                format!("{} {}", view_model.method(), view_model.effective_url()),
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
                    .debug_selector(|| "redirect-configuration".into())
                    .h(px(82.0))
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
                                    .child("Redirect policy"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Captured per request"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.render_redirect_policy_button(
                                RedirectPolicy::Follow,
                                "Follow",
                                "redirect-policy-follow",
                                redirect_policy == RedirectPolicy::Follow,
                                window,
                                cx,
                            ))
                            .child(self.render_redirect_policy_button(
                                RedirectPolicy::DoNotFollow,
                                "Do not follow",
                                "redirect-policy-do-not-follow",
                                redirect_policy == RedirectPolicy::DoNotFollow,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .ml_3()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .font_family(FONT_UI)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Maximum redirects"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(self.render_redirect_stepper(
                                        -1,
                                        "Decrease maximum redirects",
                                        "redirect-max-hops-decrease",
                                        redirect_policy == RedirectPolicy::Follow,
                                        window,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .debug_selector(|| "redirect-max-hops-input".into())
                                            .h(px(30.0))
                                            .w(px(70.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .px_2()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(if redirect_policy
                                                == RedirectPolicy::Follow
                                            {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(PANEL))
                                            .opacity(if redirect_policy == RedirectPolicy::Follow {
                                                1.0
                                            } else {
                                                0.55
                                            })
                                            .child(self.max_redirects_input.clone()),
                                    )
                                    .child(self.render_redirect_stepper(
                                        1,
                                        "Increase maximum redirects",
                                        "redirect-max-hops-increase",
                                        redirect_policy == RedirectPolicy::Follow,
                                        window,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(move || {
                                if redirect_policy == RedirectPolicy::Follow {
                                    "redirect-policy-follow-active".into()
                                } else {
                                    "redirect-policy-do-not-follow-active".into()
                                }
                            })
                            .ml_auto()
                            .h(px(26.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .bg(rgb(if redirect_policy == RedirectPolicy::Follow {
                                OK_SOFT
                            } else {
                                INFO_SOFT
                            }))
                            .font_family(FONT_MONO)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(if redirect_policy == RedirectPolicy::Follow {
                                OK
                            } else {
                                INFO
                            }))
                            .child(if redirect_policy == RedirectPolicy::Follow {
                                format!("Follow · max {max_redirect_hops}")
                            } else {
                                "Return first 3xx".to_string()
                            }),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "effective-redirect-preview".into())
                    .h(px(36.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(INFO_SOFT))
                    .font_family(FONT_MONO)
                    .text_size(px(10.0))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_color(rgb(TEXT))
                            .child(effective_request),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(INFO))
                            .child(if redirect_policy == RedirectPolicy::Follow {
                                format!("Follow · max_hops={max_redirect_hops}")
                            } else {
                                "Do not follow · first 3xx".to_string()
                            }),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "redirect-options-contract".into())
                    .px_3()
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        "Follow resolves relative and absolute Location values. Do not follow preserves the first 3xx response.",
                    ),
            )
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
