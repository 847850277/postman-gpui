use super::{navigation::AppRoute, PostmanApp};
use crate::ui::theme::{
    self, metrics as m, ACCENT, ACCENT_SOFT, FONT_MONO, INFO, LINE, LINE_STRONG, MUTED, PANEL,
    PANEL_ALT, SUBTEXT, TEXT,
};
use gpui::{
    div, prelude::FluentBuilder, rems, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        Icon,
    },
};

impl PostmanApp {
    pub(super) fn render_home(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Resolve responsive geometry against the actual content width, including app zoom.
        let available = window.viewport_size().width / window.rem_size() - 4.5;
        let inset = (available * 0.05).clamp(1.25, 5.25);
        let stacked = available - inset * 2. < 56.;
        let short = window.viewport_size().height / window.rem_size() < 44.;
        let recent = self
            .view_model
            .read(cx)
            .recent_requests()
            .take(3)
            .map(|request| {
                (
                    request.tab_id(),
                    request.method(),
                    request.tab_title(),
                    request.url().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let eyebrow = div()
            .font_family(FONT_MONO)
            .text_size(rems(11. / 16.))
            .line_height(gpui::relative(14. / 11.))
            .text_color(ACCENT.resolve(cx))
            .mb_4()
            .child("POSTMAN / GPUI");
        let heading = div()
            .debug_selector(|| "home-title".into())
            .text_size(rems(if stacked { 2. } else { 2.625 }))
            .line_height(gpui::relative(1.15))
            .font_weight(m::SEMIBOLD)
            .child("One request. Or the whole flow.");
        let subtitle = div()
            .mt(rems(14. / 16.))
            .mb(rems(if short { 1.5 } else { 34. / 16. }))
            .text_size(rems(14. / 16.))
            .line_height(gpui::relative(1.7))
            .text_color(SUBTEXT.resolve(cx))
            .child("Choose how you want to work. Your requests and flows are always a click away.");
        let cards = div()
            .flex()
            .gap(rems(22. / 16.))
            .when(stacked, |row| row.flex_col())
            .child(self.mode_card(AppRoute::Http, stacked, cx))
            .child(self.mode_card(AppRoute::Flows, stacked, cx));
        let recent_heading = div()
            .mt(rems(if short { 1.5 } else { 2.25 }))
            .flex()
            .items_center()
            .child(
                div()
                    .text_size(rems(14. / 16.))
                    .font_weight(m::SEMIBOLD)
                    .child("Continue editing"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(rems(11. / 16.))
                    .text_color(MUTED.resolve(cx))
                    .child("In this session"),
            );
        let recent_columns = div()
            .mt(rems(14. / 16.))
            .flex()
            .gap(rems(22. / 16.))
            .when(stacked, |row| row.flex_col())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(group_label("HTTP REQUESTS", cx))
                    .when(recent.is_empty(), |column| {
                        column.child(
                            div()
                                .debug_selector(|| "home-empty-requests".into())
                                .py_3()
                                .text_size(m::LABEL)
                                .text_color(MUTED.resolve(cx))
                                .child("No requests to continue. Open the HTTP editor to start."),
                        )
                    })
                    .children(recent.into_iter().map(|(id, method, title, url)| {
                        Button::new(("recent-request", id.0))
                            .ghost()
                            .debug_selector(move || format!("recent-request-{id}"))
                            .accessibility_label(format!("Continue {method} {title}"))
                            .w_full()
                            .min_w_0()
                            .h_16()
                            .px_0()
                            .border_b_1()
                            .border_color(LINE.resolve(cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.resume_request(id, window, cx)
                            }))
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .text_color(TEXT.resolve(cx))
                                    .child(
                                        div()
                                            .w_10()
                                            .flex_none()
                                            .font_family(FONT_MONO)
                                            .text_size(m::CAPTION)
                                            .text_color(theme::method_color(method).resolve(cx))
                                            .child(method.to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .items_start()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .truncate()
                                                    .text_size(m::LABEL)
                                                    .font_weight(m::MEDIUM)
                                                    .child(title),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .truncate()
                                                    .text_size(m::CAPTION)
                                                    .text_color(MUTED.resolve(cx))
                                                    .child(if url.is_empty() {
                                                        "No URL yet".into()
                                                    } else {
                                                        url
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(m::CAPTION)
                                            .text_color(MUTED.resolve(cx))
                                            .child("Open"),
                                    ),
                            )
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(group_label("FLOWS", cx))
                    .child(div().py_3().child(super::flows::empty_message(cx))),
            );

        let content = div().debug_selector(|| "home-inner".into())
            .w_full().max_w(rems(66.25)).mx_auto().flex().flex_col()
            .child(eyebrow).child(heading).child(subtitle).child(cards)
            .child(recent_heading).child(recent_columns)
            .child(div().mt_7().text_size(rems(11. / 16.)).text_color(MUTED.resolve(cx))
                .child("Appearance is shared across HTTP and Flows. Drafts remain open while you switch."));
        let vertical_inset = if short {
            1.75
        } else {
            (window.viewport_size().height / window.rem_size() * 0.05).clamp(1.75, 3.75)
        };
        div()
            .id("home-screen")
            .debug_selector(|| "home-screen".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.home_scroll)
            .bg(PANEL.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .text_size(m::BODY)
            .font_weight(gpui::FontWeight::NORMAL)
            .line_height(gpui::relative(1.5))
            .child(
                div()
                    .px(rems(inset))
                    .py(rems(vertical_inset))
                    .child(content),
            )
    }

    fn mode_card(
        &self,
        route: AppRoute,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let http = route == AppRoute::Http;
        let id = if http {
            "home-open-http"
        } else {
            "home-open-flows"
        };
        let color = if http { ACCENT } else { INFO };
        // Kit Base owns native pointer/keyboard activation. The styled Button wraps
        // content as a single-line label and fixes hover styling in 0.7.1; cards need
        // wrapping content and the prototype's quiet border-only hover treatment.
        let button = gpui_kit::base::Button::new(id)
            .debug_selector(move || id.into())
            .border_1()
            .rounded(m::DIALOG_RADIUS)
            .accessibility_label(if http {
                "Open HTTP editor"
            } else {
                "Open Flows"
            })
            .flex_1()
            .min_w_0()
            .w_full()
            .h_auto()
            .p(rems(25. / 16.))
            .bg(PANEL.resolve(cx))
            .border_color(LINE.resolve(cx))
            .hover(|style| {
                style
                    .bg(PANEL.resolve(cx))
                    .border_color(LINE_STRONG.resolve(cx))
            })
            .focus_visible(|style| style.border_color(ACCENT.resolve(cx)))
            .active(|style| style.bg(PANEL_ALT.resolve(cx)))
            .on_click(cx.listener(move |this, _, window, cx| this.navigate(route, window, cx)));
        let content = div().w_full().min_w_0().flex().flex_col().items_start().whitespace_normal().line_height(gpui::relative(1.5)).text_color(TEXT.resolve(cx))
                .child(div().w_full().flex().items_center().justify_between()
                    .child(div().size(rems(42./16.)).rounded_lg().border_1().border_color(LINE.resolve(cx)).bg(if http {ACCENT_SOFT} else {PANEL_ALT}.resolve(cx)).flex().items_center().justify_center()
                        .child(Icon::new(if http {IconName::Terminal} else {IconName::Workflow}).size(m::ICON).text_color(color.resolve(cx))))
                    .child(div().font_family(FONT_MONO).text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child(if http {"01 / REQUEST"} else {"02 / ORCHESTRATE"})))
                .child(div().mt_5().mb_2().text_size(rems(1.5)).font_weight(m::SEMIBOLD).child(route.label()))
                .child(div().text_size(m::LABEL).font_weight(gpui::FontWeight::NORMAL).line_height(gpui::relative(1.8)).text_color(SUBTEXT.resolve(cx))
                    .child(if http {"Compose, send, and inspect an API request.\nStart from a URL in the HTTP editor."} else {"Connect requests into a repeatable sequence.\nFlow editing is not available in this version."}))
                .when(!compact, |card| card.child(div().w_full().h(rems(135./16.)).mt(rems(22./16.)).p(rems(14./16.)).rounded(m::URL_RADIUS).border_1().border_color(LINE.resolve(cx)).bg(PANEL_ALT.resolve(cx))
                    .child(if http {
                        div().flex().flex_col().gap_3().font_family(FONT_MONO).text_size(m::CAPTION).font_weight(gpui::FontWeight::NORMAL)
                            .child(div().pb_3().border_b_1().border_color(LINE.resolve(cx)).text_color(SUBTEXT.resolve(cx)).child("METHOD   URL"))
                            .child(div().text_color(MUTED.resolve(cx)).child("Compose → Send → Inspect"))
                            .child(div().text_color(MUTED.resolve(cx)).child("Your response appears after sending.")).into_any_element()
                    } else {
                        div().size_full().flex().items_center().justify_center()
                            .children(["Request", "Extract", "Next step"].into_iter().enumerate().map(|(i, label)| div().flex().items_center().min_w_0()
                                .when(i > 0, |row| row.child(Icon::new(IconName::ArrowRight).size(m::SMALL_ICON).text_color(MUTED.resolve(cx))))
                                .child(div().p_3().border_1().border_color(LINE_STRONG.resolve(cx)).rounded(m::RADIUS).bg(PANEL.resolve(cx)).text_size(m::CAPTION).child(label)))).into_any_element()
                    })))
                .child(div().mt_5().flex().items_center().gap_2().text_size(m::LABEL).font_weight(m::MEDIUM).text_color(color.resolve(cx))
                    .child(if http {"Open HTTP editor"} else {"Open Flows"})
                    .child(Icon::new(IconName::ArrowRight).size(m::SMALL_ICON)));
        button.child(content)
    }
}

fn group_label(label: &'static str, cx: &gpui::App) -> impl IntoElement {
    div()
        .font_family(FONT_MONO)
        .text_size(m::CAPTION)
        .text_color(MUTED.resolve(cx))
        .child(label)
}
