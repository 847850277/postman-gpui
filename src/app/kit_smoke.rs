//! Small native integration surface opened with `--kit-smoke` during the P0 migration.

use gpui::{
    div, px, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        dialog::{DialogClose, DialogFooter},
        input::{Input, InputState},
        ActiveTheme, WindowExt,
    },
    TestSupportExt,
};

use crate::ui::theme::{FONT_MONO, FONT_UI};

pub struct KitSmokeView {
    input: Entity<InputState>,
}

impl KitSmokeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Type a message, then open the dialog")
            }),
        }
    }
}

impl Render for KitSmokeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("kit-smoke")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(FONT_UI)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_8()
            .child(
                div()
                    .w_full()
                    .max_w(px(560.))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("GPUI Kit compatibility"),
                    )
                    .child("Native input, keyboard navigation, fonts, and dialog overlays.")
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_sm()
                            .child("JetBrains Mono · GET /health · 200 OK"),
                    )
                    .child(
                        div().flex().flex_col().gap_2().child("Message").child(
                            Input::new(&self.input)
                                .id("kit-smoke-input")
                                .aria_label("Message"),
                        ),
                    )
                    .child(
                        div().child(
                            Button::new("kit-smoke-open")
                                .primary()
                                .label("Open dialog")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let value = this.input.read(cx).value();
                                    window.open_dialog(cx, move |dialog, _, _| {
                                        dialog
                                            .title("Input preview")
                                            .child("The current input value:")
                                            .child(
                                                div()
                                                    .id("kit-smoke-value")
                                                    .test_support()
                                                    .aria_label(value.clone())
                                                    .font_family(FONT_MONO)
                                                    .child(value.clone()),
                                            )
                                            .footer(
                                                DialogFooter::new().child(
                                                    DialogClose::new()
                                                        .trigger(|button| button.label("Close")),
                                                ),
                                            )
                                    });
                                })),
                        ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Tab / Shift+Tab to move focus · Enter to open · Esc to close"),
                    ),
            )
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
