use crate::ui::theme::FONT_MONO;
use gpui::{
    actions, div, prelude::*, App, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyBinding, Render, SharedString, Styled, Subscription, Window,
};
use gpui_kit::component::{
    input::{Input, InputEvent, InputState},
    Sizable,
};

actions!(header_input, [FocusNext, FocusPrevious]);

#[derive(Debug, Clone)]
pub enum HeaderInputEvent {
    ValueChanged(String),
    SubmitRequested,
}

/// Header/auth/search presentation around a retained Kit single-line editor.
pub struct HeaderInput {
    input: Entity<InputState>,
    // Kit also emits Change after Enter. This snapshot suppresses unchanged domain events;
    // InputState remains the sole editable buffer.
    last_value: SharedString,
    embedded: bool,
    font_family: &'static str,
    _subscription: Subscription,
}

impl HeaderInput {
    pub fn new(
        placeholder: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(placeholder.into(), false, window, cx)
    }

    /// Masks text and disables Kit's clipboard Copy/Cut capabilities while retaining the value.
    pub fn new_masked(
        placeholder: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(placeholder.into(), true, window, cx)
    }

    fn build(
        placeholder: String,
        masked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(masked)
        });
        let subscription = cx.subscribe(&input, Self::on_input_event);
        Self {
            input,
            last_value: SharedString::default(),
            embedded: false,
            font_family: FONT_MONO,
            _subscription: subscription,
        }
    }

    fn on_input_event(
        &mut self,
        input: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let value = input.read(cx).value();
                if value != self.last_value {
                    self.last_value = value.clone();
                    cx.emit(HeaderInputEvent::ValueChanged(value.to_string()));
                }
            }
            InputEvent::PressEnter {
                secondary: false,
                shift: false,
            } => cx.emit(HeaderInputEvent::SubmitRequested),
            _ => {}
        }
    }

    /// Lets a parent component provide the field background, border, radius, and padding.
    pub fn with_embedded_chrome(mut self, embedded: bool) -> Self {
        self.embedded = embedded;
        self
    }

    pub fn with_font_family(mut self, font_family: &'static str) -> Self {
        self.font_family = font_family;
        self
    }

    /// Projects contextual placeholder copy without treating it as an input edit.
    pub fn project_placeholder(
        &mut self,
        placeholder: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder.into(), window, cx)
        });
    }

    /// Silent model projection. Always resets editing history, including equal-valued requests.
    /// Call at the owner's projection boundary, never on every render.
    pub fn project_content(
        &mut self,
        content: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_value = self.input.update(cx, |input, cx| {
            input.set_value(content.into(), window, cx);
            input.value()
        });
    }
}

impl EventEmitter<HeaderInputEvent> for HeaderInput {}

impl Focusable for HeaderInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for HeaderInput {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("HeaderInput")
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx))
            .child(
                Styled::h_full(Input::new(&self.input))
                    .small()
                    .appearance(!self.embedded)
                    .font_family(self.font_family)
                    .text_xs()
                    .when(self.embedded, |input| input.p_0()),
            )
    }
}

/// Only logical traversal belongs to the application; Kit owns all text-editing bindings.
pub fn setup_header_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("tab", FocusNext, Some("HeaderInput > Input")),
        KeyBinding::new("shift-tab", FocusPrevious, Some("HeaderInput > Input")),
    ]
}

#[cfg(test)]
mod tests {
    use super::{HeaderInput, HeaderInputEvent};
    use gpui::{
        AnyWindowHandle, AppContext, ClipboardItem, Entity, Focusable, TestAppContext,
        WindowOptions,
    };
    use gpui_kit::{
        component::input::{Copy, Cut, Paste, SelectAll},
        test::TestWindowExt,
    };
    use std::{cell::RefCell, rc::Rc};

    fn open_input(cx: &mut TestAppContext, masked: bool) -> (AnyWindowHandle, Entity<HeaderInput>) {
        cx.update(crate::ui::kit::init);
        cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    if masked {
                        HeaderInput::new_masked("Secret", window, cx)
                    } else {
                        HeaderInput::new("Value", window, cx)
                    }
                })
            })
        })
        .unwrap()
    }

    #[gpui_kit::test]
    fn edits_emit_only_changes_and_equal_projection_silently_resets_undo(cx: &mut TestAppContext) {
        let (handle, input) = open_input(cx, false);
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&input, move |_, event, _| {
                observed.borrow_mut().push(event.clone());
            })
        });
        cx.update_window(handle, |_, window, cx| {
            let state = input.read(cx).input.clone();
            let focus = input.read(cx).focus_handle(cx);
            assert_eq!(focus, state.read(cx).focus_handle(cx));
            focus.focus(window, cx);
            window.render_frame(cx);
        })
        .unwrap();
        for text in ["A", "😀", "中"] {
            cx.update_window(handle, |_, window, cx| window.input(text, cx))
                .unwrap();
        }
        cx.update_window(handle, |_, window, cx| {
            window.press("enter", cx);
        })
        .unwrap();
        assert!(
            matches!(
                events.borrow().as_slice(),
                [HeaderInputEvent::ValueChanged(first), HeaderInputEvent::ValueChanged(second),
                    HeaderInputEvent::ValueChanged(third), HeaderInputEvent::SubmitRequested]
                    if first == "A" && second == "A😀" && third == "A😀中"
            ),
            "unexpected events: {:?}",
            events.borrow()
        );
        events.borrow_mut().clear();

        cx.update_window(handle, |_, window, cx| {
            let state = input.read(cx).input.clone();
            input.update(cx, |input, cx| {
                input.project_placeholder("New context", window, cx);
                input.project_content("A😀中", window, cx);
            });
            assert_eq!(input.read(cx).input, state);
            window.press("ctrl-z", cx);
            assert_eq!(state.read(cx).value().as_ref(), "A😀中");
            assert_eq!(
                state.read(cx).presentation().placeholder().as_ref(),
                "New context"
            );
        })
        .unwrap();
        assert!(events.borrow().is_empty());

        cx.update_window(handle, |_, window, cx| {
            window.input("!", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.press("ctrl-z", cx);
            assert_eq!(input.read(cx).input.read(cx).value().as_ref(), "A😀中");
        })
        .unwrap();
        assert!(matches!(
            events.borrow().as_slice(),
            [HeaderInputEvent::ValueChanged(edited), HeaderInputEvent::ValueChanged(undone)]
                if edited == "A😀中!" && undone == "A😀中"
        ));
    }

    #[gpui_kit::test]
    fn masked_input_disables_native_copy_cut_but_keeps_paste_and_real_values(
        cx: &mut TestAppContext,
    ) {
        let (handle, input) = open_input(cx, true);
        cx.update_window(handle, |_, window, cx| {
            input.update(cx, |input, cx| {
                input.project_content("secret😀", window, cx)
            });
            let state = input.read(cx).input.clone();
            let focus = input.read(cx).focus_handle(cx);
            focus.focus(window, cx);
            window.render_frame(cx);
            focus.dispatch_action(&SelectAll, window, cx);
            window.render_frame(cx);
            let capabilities = state.read(cx).context_menu_capabilities();
            assert!(capabilities.is_masked());
            assert!(capabilities.has_selection());
            assert!(capabilities.is_editable());
            assert!(!capabilities.is_copyable());

            cx.write_to_clipboard(ClipboardItem::new_string("replacement".into()));
            focus.dispatch_action(&Copy, window, cx);
            focus.dispatch_action(&Cut, window, cx);
            window.render_frame(cx);
            assert_eq!(state.read(cx).value().as_ref(), "secret😀");
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("replacement")
            );

            focus.dispatch_action(&Paste, window, cx);
            window.render_frame(cx);
            assert_eq!(state.read(cx).value().as_ref(), "replacement");
        })
        .unwrap();
    }
}
