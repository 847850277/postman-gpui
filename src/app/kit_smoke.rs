//! Small native validation surface opened with `--kit-smoke`.
use crate::{
    app::appearance::Appearance,
    ui::{
        components::kit_controls as controls,
        theme::{metrics as m, FONT_MONO, FONT_UI},
    },
};
use gpui::{
    div, prelude::FluentBuilder, rems, AppContext, Context, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_kit::{
    component::{
        button::ButtonVariants,
        checkbox::Checkbox,
        dialog::{DialogClose, DialogFooter},
        input::{Editor, EditorState, Input, InputState},
        searchable_list::SearchableVec,
        select::SelectState,
        ActiveTheme, Disableable, IndexPath, WindowExt,
    },
    TestSupportExt,
};

pub struct KitSmokeView {
    input: Entity<InputState>,
    url: Entity<InputState>,
    search: Entity<InputState>,
    method: Entity<controls::MethodState>,
    key: Entity<InputState>,
    value: Entity<InputState>,
    code: Entity<EditorState>,
    tab: usize,
    enabled: bool,
    disabled: bool,
    actions: usize,
    _subscriptions: Vec<Subscription>,
}
impl KitSmokeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("https://api.acme.dev/v1/users?page=1&limit=10")
        });
        let subscription = cx.observe(&url, |_, _, cx| cx.notify());
        let method = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(vec![
                    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS",
                ]),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let method_subscription = cx.observe(&method, |_, _, cx| cx.notify());
        Self {
            input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Type a message, then open the dialog")
            }),
            url,
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Filter history…")),
            method,
            key: cx.new(|cx| InputState::new(window, cx).default_value("page")),
            value: cx.new(|cx| InputState::new(window, cx).default_value("1")),
            code: cx.new(|cx| {
                EditorState::new(window, cx).language("json").default_value(
                    "{\n  \"name\": \"Maya Chen\",\n  \"active\": true,\n  \"page\": 1\n}",
                )
            }),
            tab: 0,
            enabled: true,
            disabled: false,
            actions: 0,
            _subscriptions: vec![subscription, method_subscription],
        }
    }
}

impl Render for KitSmokeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().height < gpui::px(700.);
        let editor_height = if compact { rems(6.) } else { rems(10.) };
        let invalid = reqwest::Url::parse(&self.url.read(cx).value()).is_err();
        let heading = div()
            .text_size(m::TITLE)
            .font_weight(FontWeight::SEMIBOLD)
            .child("UI foundations");
        let description = div()
            .text_size(m::LABEL)
            .text_color(cx.theme().muted_foreground)
            .child("Native Kit controls · HTTP design reference · No requests are sent");
        let header = div()
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().child(heading).child(description))
            .child(super::appearance::button("kit-theme", cx));
        let action_controls = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div().w(rems(28.)).min_w_0().child(controls::search_input(
                    "kit-search-group",
                    Input::new(&self.search)
                        .id("kit-search")
                        .aria_label("Filter history"),
                    &self.search,
                    window,
                    cx,
                )),
            )
            .child(
                controls::button("kit-action", "Action")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.actions += 1;
                        cx.notify();
                    })),
            )
            .child(
                controls::button("kit-disabled", "Disabled")
                    .disabled(true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.actions += 1;
                        cx.notify();
                    })),
            )
            .child(
                controls::button("kit-loading", "Loading")
                    .icon(gpui_kit::component::IconName::Loader)
                    .loading(true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.actions += 1;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("kit-action-count")
                    .test_support()
                    .aria_label(format!("{} actions", self.actions))
                    .child(format!("{} actions", self.actions)),
            );
        let dialog_controls = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                Input::new(&self.input)
                    .id("kit-smoke-input")
                    .aria_label("Message")
                    .h(m::CONTROL)
                    .text_size(m::LABEL),
            )
            .child(
                controls::button("kit-smoke-open", "Open dialog")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let value = this.input.read(cx).value();
                        window.open_dialog(cx, move |dialog, window, cx| {
                            controls::dialog(dialog, "Input preview", window, cx)
                                .child("The current input value:")
                                .child(
                                    div()
                                        .id("kit-smoke-value")
                                        .test_support()
                                        .aria_label(value.clone())
                                        .font_family(FONT_MONO)
                                        .child(value.clone()),
                                )
                                .child(super::appearance::button("kit-dialog-theme", cx))
                                .footer(DialogFooter::new().child(
                                    DialogClose::new().trigger(|button| button.label("Close")),
                                ))
                        });
                    })),
            );
        let parameter_table = div()
            .rounded(m::RADIUS)
            .border_1()
            .border_color(cx.theme().border)
            .overflow_hidden()
            .child(
                div()
                    .h(m::TABLE_HEADER)
                    .flex()
                    .items_center()
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
                    .text_size(m::CAPTION)
                    .child(div().w_10())
                    .child(div().flex_1().px_3().child("KEY"))
                    .child(div().flex_1().px_3().child("VALUE")),
            )
            .child(controls::editable_row(
                "kit-row",
                controls::parameter_checkbox("kit-row-enabled", self.enabled, cx)
                    .accessibility_label("Enable parameter")
                    .on_change({
                        let view = cx.entity().downgrade();
                        move |state, _, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.enabled = state == gpui_kit::base::CheckboxState::Checked;
                                cx.notify();
                            });
                        }
                    }),
                Input::new(&self.key)
                    .id("kit-row-key")
                    .aria_label("Parameter key")
                    .disabled(!self.enabled),
                Input::new(&self.value)
                    .id("kit-row-value")
                    .aria_label("Parameter value")
                    .disabled(!self.enabled),
                cx,
            ));
        let viewport = window.viewport_size();
        let help = div()
            .text_size(m::LABEL)
            .text_color(cx.theme().muted_foreground)
            .child(format!(
                "Tab / Shift+Tab to move focus · Enter to open · Esc to close · {:.0} × {:.0} logical px · {:.1}× scale",
                f32::from(viewport.width),
                f32::from(viewport.height),
                window.scale_factor()
            ));
        let code_editor = div().id("kit-editor").h(editor_height).flex_none().child(
            Editor::new(&self.code)
                .h(editor_height)
                .aria_label("JSON body")
                .text_size(m::CODE)
                .line_height(gpui::relative(m::CODE_LINE_HEIGHT)),
        );
        let section_heading = div()
            .flex()
            .items_center()
            .justify_between()
            .text_size(m::LABEL)
            .child(
                [
                    "Query parameters",
                    "Request headers",
                    "Request body",
                    "Authorization",
                ][self.tab],
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Editable row and code input"),
            );
        let state_controls = div()
            .flex()
            .items_center()
            .gap_4()
            .child(
                Checkbox::new("kit-disable-url")
                    .accessibility_label("Disable URL group")
                    .label("Disable URL group")
                    .checked(self.disabled)
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.disabled = *checked;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("kit-url-error")
                    .test_support()
                    .aria_label(if invalid {
                        "Enter a valid URL"
                    } else {
                        "Valid URL"
                    })
                    .text_size(m::LABEL)
                    .text_color(if invalid {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(if invalid {
                        "Enter a valid URL"
                    } else {
                        "URL and method share one focus boundary"
                    }),
            );
        div()
            .id("kit-smoke")
            .size_full()
            .overflow_y_scroll()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(FONT_UI)
            .text_size(m::BODY)
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .when(compact, |view| view.p_4().gap_3())
            .child(header)
            .when_some(Appearance::error(cx).map(str::to_owned), |view, error| {
                view.child(div().text_color(cx.theme().danger).child(error))
            })
            .child(controls::request_url(
                "kit-url-group",
                Input::new(&self.url)
                    .id("kit-url")
                    .aria_label("Request URL"),
                &self.url,
                &self.method,
                invalid,
                self.disabled,
                window,
                cx,
            ))
            .child(state_controls)
            .child(
                controls::tabs("kit-tabs", &["Params", "Headers", "Body", "Auth"], self.tab)
                    .on_click(cx.listener(|this, index, _, cx| {
                        this.tab = *index;
                        cx.notify();
                    })),
            )
            .child(section_heading)
            .child(parameter_table)
            .child(code_editor)
            .child(action_controls)
            .child(dialog_controls)
            .child(help)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ElementInputHandler, InputHandler, TestAppContext, WindowOptions};
    use gpui_kit::test::TestWindowExt;

    // Exercise the public IME protocol against the rendered input. OS candidate-window
    // selection still needs a native input-method check on each supported platform.
    #[gpui::test]
    fn kit_input_composes_and_commits_chinese_after_an_astral_character(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (handle, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| KitSmokeView::new(window, cx))
                })
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.click("kit-smoke-input", cx);
            window.input("A🦀", cx);
            let mut handler = ElementInputHandler::new(
                window.find("kit-smoke-input").bounds(),
                view.read(cx).input.clone(),
            );
            handler.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            assert_eq!(handler.marked_text_range(window, cx), Some(3..5));
            handler.replace_and_mark_text_in_range(None, "你好", Some(2..2), window, cx);
            window.render_frame(cx);
            assert_eq!(window.find("kit-smoke-input").value(), Some("A🦀你好"));
            handler.replace_text_in_range(None, "你好", window, cx);
            assert_eq!(handler.marked_text_range(window, cx), None);
            window.render_frame(cx);
            assert_eq!(window.find("kit-smoke-input").value(), Some("A🦀你好"));
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-z"
                } else {
                    "ctrl-z"
                },
                cx,
            );
            assert_eq!(window.find("kit-smoke-input").value(), Some("A🦀"));
        })
        .unwrap();
    }
}
