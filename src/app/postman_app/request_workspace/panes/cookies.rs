use crate::{
    app::WorkspaceViewModel,
    ui::theme::{
        ACCENT, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL,
        PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, Context, Entity, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};

#[derive(Clone, Debug)]
pub(in crate::app::postman_app::request_workspace) enum CookiePaneEvent {
    ClearAllRequested,
}

/// Application-session cookie controls. Sensitive values remain in the transport jar; this pane
/// renders only the non-sensitive projection owned by WorkspaceViewModel.
pub(in crate::app::postman_app::request_workspace) struct CookiePane {
    view_model: Entity<WorkspaceViewModel>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<CookiePaneEvent> for CookiePane {}

impl CookiePane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.observe(&view_model, |_, _, cx| cx.notify())];
        Self {
            view_model,
            _subscriptions: subscriptions,
        }
    }

    fn clear_all(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(CookiePaneEvent::ClearAllRequested);
    }
}

impl Render for CookiePane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                                    .child("Application Cookie Jar"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "cookie-jar-scope".into())
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child(
                                        "In-memory session · values protected · shared by requests",
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
                            .child("Clear all cookies")
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::clear_all)),
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
