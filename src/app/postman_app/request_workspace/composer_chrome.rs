use super::composer::RequestComposer;
use crate::{
    app::{ActivateControl, RequestPane},
    ui::{
        components::kit_controls,
        theme::{metrics as m, ACCENT, LINE, MUTED, PANEL, PANEL_ALT, TEXT},
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, rems, Context, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{
    base::Tab,
    component::{input::Input, scroll::ScrollableElement},
};

actions!(request_pane_tabs, [NextRequestPane, PreviousRequestPane]);
pub(super) fn setup_request_pane_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("right", NextRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("down", NextRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("left", PreviousRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("up", PreviousRequestPane, Some("RequestPaneTab")),
    ]
}

impl RequestComposer {
    fn request_tab(
        &self,
        pane: RequestPane,
        label: &'static str,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self
            .view_model
            .read(cx)
            .active_request()
            .is_some_and(|r| r.request_pane() == pane);
        let id = request_pane_selector(pane);
        let focus = self.request_pane_focus_handles[request_pane_index(pane)].clone();
        let mouse_focus = focus.clone();
        // Kit 0.7.1 Base Tab supplies semantics/pointer activation; compound arrow-key
        // navigation is not yet supplied upstream, so retain the application's tab actions.
        Tab::new(id)
            .debug_selector(move || id.into())
            .selected(active)
            .accessibility_label(format!("{label} request pane"))
            .track_focus(&focus)
            .key_context("KeyboardButton RequestPaneTab")
            .h(m::PANE_TAB)
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .border_b_2()
            .border_color(if active {
                ACCENT.resolve(cx)
            } else {
                gpui::rgba(0)
            })
            .text_size(m::LABEL)
            .font_weight(m::MEDIUM)
            .text_color(if active { ACCENT } else { MUTED }.resolve(cx))
            .focus_visible(|s| s.bg(PANEL_ALT.resolve(cx)).border_color(ACCENT.resolve(cx)))
            .child(label)
            .when_some(count.filter(|n| *n > 0), |tab, n| {
                tab.child(
                    div()
                        .px_1()
                        .rounded_sm()
                        .bg(PANEL_ALT.resolve(cx))
                        .text_size(m::CAPTION)
                        .child(n.to_string()),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                mouse_focus.focus(window, cx);
                this.set_request_pane(pane, cx);
            }))
            .on_action(
                cx.listener(move |this, _: &ActivateControl, _, cx| {
                    this.set_request_pane(pane, cx)
                }),
            )
            .on_action(cx.listener(move |this, _: &NextRequestPane, window, cx| {
                this.activate_relative_request_pane(pane, 1, window, cx)
            }))
            .on_action(
                cx.listener(move |this, _: &PreviousRequestPane, window, cx| {
                    this.activate_relative_request_pane(pane, -1, window, cx)
                }),
            )
    }

    fn activate_relative_request_pane(
        &mut self,
        pane: RequestPane,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = (request_pane_index(pane) as isize + delta)
            .rem_euclid(REQUEST_PANES.len() as isize) as usize;
        self.pane_tabs_scroll.scroll_to_item(next);
        self.request_pane_focus_handles[next].focus(window, cx);
        self.set_request_pane(REQUEST_PANES[next], cx);
    }

    pub(super) fn render_request_head(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let model = self.view_model.read(cx);
        let active = model.active_request();
        let sending = active.is_some_and(|r| r.is_sending());
        let title = active
            .map(|r| r.tab_title())
            .unwrap_or_else(|| "Untitled request".into());
        let compact = window.viewport_size().height < gpui::px(700.);
        div()
            .debug_selector(|| "request-head".into())
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .gap(if compact { rems(0.75) } else { rems(23. / 16.) })
            .px(rems(
                (window.viewport_size().width.as_f32() * 0.02).clamp(12., 36.) / 16.,
            ))
            .py(rems(
                if compact {
                    12.
                } else {
                    (window.viewport_size().height.as_f32() * 0.022).clamp(12., 22.)
                } / 16.,
            ))
            .font_weight(gpui::FontWeight::NORMAL)
            .bg(PANEL.resolve(cx))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .debug_selector(|| "request-title".into())
                            .truncate()
                            .text_size(if compact { rems(22. / 16.) } else { m::TITLE })
                            .line_height(if compact {
                                rems(27.5 / 16.)
                            } else {
                                rems(32.5 / 16.)
                            })
                            .font_weight(m::SEMIBOLD)
                            .text_color(TEXT.resolve(cx))
                            .child(title),
                    )
                    .when(!compact, |h| {
                        h.child(
                            div()
                                .text_size(m::LABEL)
                                .line_height(rems(1.125))
                                .text_color(MUTED.resolve(cx))
                                .child("Configure and send an HTTP request."),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(10. / 16.))
                    .h(m::URL)
                    .child(
                        div()
                            .id("request-url-container")
                            .key_context("RequestUrl")
                            .debug_selector(|| "url-input".into())
                            .flex_1()
                            .min_w_0()
                            .child(kit_controls::request_url(
                                "request-url-group",
                                Input::new(&self.url_input).id("request-url-input"),
                                &self.url_input,
                                &self.method_selector,
                                false,
                                false,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        kit_controls::editor_primary_button("send-button", "", cx)
                            .debug_selector(|| "send-button".into())
                            .track_focus(&self.send_focus_handle)
                            .accessibility_label(if sending {
                                "Cancel active request"
                            } else {
                                "Send active request"
                            })
                            .bg(if sending {
                                crate::ui::theme::ERROR
                            } else {
                                ACCENT
                            }
                            .resolve(cx))
                            .text_color(crate::ui::theme::ON_ACCENT.resolve(cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(!sending, |d| {
                                        d.child(
                                            gpui_kit::component::Icon::new(
                                                gpui_kit::assets::IconName::Send,
                                            )
                                            .size(m::SMALL_ICON),
                                        )
                                    })
                                    .child(if sending { "Cancel" } else { "Send" })
                                    .when(!sending, |d| {
                                        d.child(div().text_size(m::CAPTION).child(
                                            if cfg!(target_os = "macos") {
                                                "⌘ ↵"
                                            } else {
                                                "Ctrl ↵"
                                            },
                                        ))
                                    }),
                            )
                            .h(m::URL)
                            .w(rems(8.))
                            .flex_none()
                            .when(sending, |b| {
                                b.child(div().debug_selector(|| "cancel-send-control".into()))
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.click_send(cx))),
                    ),
            )
    }

    pub(super) fn render_request_menu(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let model = self.view_model.read(cx);
        let request = model.active_request();
        let params = request.map_or(0, |r| r.enabled_param_count());
        let headers = request.map_or(0, |r| r.headers().iter().filter(|h| h.enabled).count());
        div()
            .id("request-pane-tabs")
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.pane_tabs_scroll)
            .horizontal_scrollbar(&self.pane_tabs_scroll)
            .h(m::PANE_TAB)
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_7()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(self.request_tab(RequestPane::Params, "Params", Some(params), cx))
            .child(self.request_tab(RequestPane::Headers, "Headers", Some(headers), cx))
            .child(self.request_tab(RequestPane::Body, "Body", None, cx))
            .child(self.request_tab(RequestPane::Authorization, "Auth", None, cx))
            .child(self.request_tab(RequestPane::Scripts, "Scripts", None, cx))
            .child(self.request_tab(RequestPane::Tests, "Tests", None, cx))
            .child(self.request_tab(RequestPane::Options, "Options", None, cx))
    }
}
const REQUEST_PANES: [RequestPane; 7] = [
    RequestPane::Params,
    RequestPane::Headers,
    RequestPane::Body,
    RequestPane::Authorization,
    RequestPane::Scripts,
    RequestPane::Tests,
    RequestPane::Options,
];
fn request_pane_index(pane: RequestPane) -> usize {
    REQUEST_PANES
        .iter()
        .position(|p| *p == pane)
        .expect("all panes have a tab")
}
fn request_pane_selector(pane: RequestPane) -> &'static str {
    match pane {
        RequestPane::Params => "request-pane-params",
        RequestPane::Headers => "request-pane-headers",
        RequestPane::Body => "request-pane-body",
        RequestPane::Authorization => "request-pane-authorization",
        RequestPane::Scripts => "request-pane-scripts",
        RequestPane::Tests => "request-pane-tests",
        RequestPane::Options => "request-pane-options",
    }
}
