use crate::{
    app::WorkspaceViewModel,
    ui::{
        components::kit_controls,
        theme::{
            ACCENT, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT,
            PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, Context, Entity, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
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
            .key_context("OverlayTrigger")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(PANEL.resolve(cx))
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
                    .border_color(LINE.resolve(cx))
                    .bg(PANEL_ALT.resolve(cx))
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
                                    .text_color(TEXT.resolve(cx))
                                    .child("Application Cookie Jar · workspace tool"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "cookie-jar-scope".into())
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(SUBTEXT.resolve(cx))
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
                            .bg(INFO_SOFT.resolve(cx))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(INFO.resolve(cx))
                            .child(format!("{count} stored")),
                    )
                    .child(
                        kit_controls::editor_button("cookie-jar-clear-all", "Clear all cookies", cx)
                            .debug_selector(|| "cookie-jar-clear-all".into())
                            .track_focus(&self.clear_focus_handle)
                            .h_8()
                            .bg(ACCENT_SOFT.resolve(cx))
                            .text_color(ACCENT.resolve(cx))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(CookiePaneEvent::ClearAllRequested)
                            })),
                    )
                    .child(
                        kit_controls::icon_button(
                            "cookie-jar-close",
                            gpui_kit::assets::IconName::Close,
                            "Close Cookie Jar",
                        )
                        .debug_selector(|| "cookie-jar-close".into())
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(CookiePaneEvent::CloseRequested)
                        })),
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
                        .bg(OK_SOFT.resolve(cx))
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(10.0))
                        .text_color(OK.resolve(cx))
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
                                .border_color(LINE.resolve(cx))
                                .bg(PANEL_ALT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(12.0))
                                .text_color(MUTED.resolve(cx))
                                .child("Cookie jar is empty")
                                .child(
                                    div().text_size(px(9.0)).text_color(SUBTEXT.resolve(cx)).child(
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
                            .border_color(LINE.resolve(cx))
                            .bg(INFO_SOFT.resolve(cx))
                            .child(
                                div()
                                    .debug_selector(move || format!("cookie-name-{index}"))
                                    .w(px(150.0))
                                    .flex_none()
                                    .font_family(FONT_MONO)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(11.0))
                                    .text_color(TEXT.resolve(cx))
                                    .child(cookie.name),
                            )
                            .child(
                                div()
                                    .debug_selector(move || format!("cookie-origin-{index}"))
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.0))
                                    .text_color(SUBTEXT.resolve(cx))
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
                                    .bg(PANEL.resolve(cx))
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(9.0))
                                    .text_color(MUTED.resolve(cx))
                                    .child("VALUE PROTECTED"),
                            )
                    })),
            )
    }
}
