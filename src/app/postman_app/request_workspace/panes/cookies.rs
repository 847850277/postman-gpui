use crate::{
    app::{ActivateControl, WorkspaceViewModel},
    ui::theme::{
        ACCENT, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL,
        PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, Context, Entity, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, Role, StatefulInteractiveElement,
    Styled, Subscription, Window,
};

#[derive(Clone, Debug)]
pub(in crate::app::postman_app) enum CookiePaneEvent {
    ClearAllRequested,
    CloseRequested,
}

/// Application-session cookie controls. Sensitive values remain in the transport jar; this pane
/// renders only the non-sensitive projection owned by WorkspaceViewModel.
pub(in crate::app::postman_app) struct CookiePane {
    view_model: Entity<WorkspaceViewModel>,
    clear_focus_handle: FocusHandle,
    close_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<CookiePaneEvent> for CookiePane {}

impl CookiePane {
    pub(in crate::app::postman_app) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.observe(&view_model, |_, _, cx| cx.notify())];
        Self {
            view_model,
            clear_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            close_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            _subscriptions: subscriptions,
        }
    }

    pub(in crate::app::postman_app) fn focus_first(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_focus_handle.focus(window, cx);
    }

    fn clear_all(
        &mut self,
        _event: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_focus_handle.focus(window, cx);
        cx.emit(CookiePaneEvent::ClearAllRequested);
    }

    fn close(&mut self, _event: &gpui::MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.blur();
        cx.emit(CookiePaneEvent::CloseRequested);
    }

    fn clear_with_keyboard(&mut self, _: &ActivateControl, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(CookiePaneEvent::ClearAllRequested);
    }

    fn close_with_keyboard(
        &mut self,
        _: &ActivateControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.blur();
        cx.emit(CookiePaneEvent::CloseRequested);
    }
}

impl Render for CookiePane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (cookies, cleared) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.cookies().to_vec(),
                view_model.last_cookie_clear_count(),
            )
        };
        let count = cookies.len();

        div()
            .debug_selector(|| "cookie-jar-panel".into())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(rgb(PANEL))
            .child(
                div()
                    .h(px(48.0))
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
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Application Cookie Jar · workspace tool"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "cookie-jar-scope".into())
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child(
                                        "Opened from the header · values protected · shared by requests",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "cookie-jar-count".into())
                            .h(px(26.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .bg(rgb(INFO_SOFT))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(INFO))
                            .child(format!("{count} stored")),
                    )
                    .child(
                        div()
                            .id("cookie-jar-clear-all")
                            .debug_selector(|| "cookie-jar-clear-all".into())
                            .track_focus(&self.clear_focus_handle)
                            .key_context("KeyboardButton OverlayTrigger")
                            .role(Role::Button)
                            .aria_label("Clear all cookies")
                            .h(px(30.0))
                            .px_3()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(rgb(ACCENT_SOFT))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(ACCENT))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x00ff_e4d5)))
                            .when(self.clear_focus_handle.is_focused(window), |button| {
                                button.border_1().border_color(rgb(ACCENT))
                            })
                            .child("Clear all cookies")
                            .on_action(cx.listener(Self::clear_with_keyboard))
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::clear_all)),
                    )
                    .child(
                        div()
                            .id("cookie-jar-close")
                            .debug_selector(|| "cookie-jar-close".into())
                            .track_focus(&self.close_focus_handle)
                            .key_context("KeyboardButton OverlayTrigger")
                            .role(Role::Button)
                            .aria_label("Close Cookie Jar")
                            .size(px(30.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(rgb(PANEL))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(16.0))
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT)))
                            .when(self.close_focus_handle.is_focused(window), |button| {
                                button.border_1().border_color(rgb(ACCENT))
                            })
                            .child("×")
                            .on_action(cx.listener(Self::close_with_keyboard))
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::close)),
                    ),
            )
            .when_some(cleared, |panel, cleared| {
                panel.child(
                    div()
                        .debug_selector(|| "cookie-jar-clear-feedback".into())
                        .h(px(28.0))
                        .px_3()
                        .flex_none()
                        .flex()
                        .items_center()
                        .rounded_lg()
                        .bg(rgb(OK_SOFT))
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(10.0))
                        .text_color(rgb(OK))
                        .child(if cleared == 1 {
                            "✓ Cleared 1 cookie".to_string()
                        } else {
                            format!("✓ Cleared {cleared} cookies")
                        }),
                )
            })
            .child(
                div()
                    .debug_selector(|| "cookie-jar-list".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when(count == 0, |list| {
                        list.child(
                            div()
                                .debug_selector(|| "cookie-jar-empty".into())
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(rgb(PANEL_ALT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(12.0))
                                .text_color(rgb(MUTED))
                                .child("Cookie jar is empty")
                                .child(
                                    div().text_size(px(9.0)).text_color(rgb(SUBTEXT)).child(
                                        "The next request sends no automatic Cookie header.",
                                    ),
                                ),
                        )
                    })
                    .children(cookies.into_iter().enumerate().map(|(index, cookie)| {
                        div()
                            .debug_selector(move || format!("cookie-row-{index}"))
                            .h(px(54.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(INFO_SOFT))
                            .child(
                                div()
                                    .debug_selector(move || format!("cookie-name-{index}"))
                                    .w(px(150.0))
                                    .flex_none()
                                    .font_family(FONT_MONO)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(11.0))
                                    .text_color(rgb(TEXT))
                                    .child(cookie.name),
                            )
                            .child(
                                div()
                                    .debug_selector(move || format!("cookie-origin-{index}"))
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child(cookie.origin),
                            )
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("cookie-value-protected-{index}")
                                    })
                                    .h(px(24.0))
                                    .px_2()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .rounded_lg()
                                    .bg(rgb(PANEL))
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(9.0))
                                    .text_color(rgb(MUTED))
                                    .child("VALUE PROTECTED"),
                            )
                    })),
            )
    }
}
