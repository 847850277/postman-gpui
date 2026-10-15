//! Shared request text surface; Kit owns the buffer, IME, selection, history and scrolling.
use crate::ui::{
    components::common::edit_context_menu::edit_popup_menu,
    theme::{metrics as m, CODE_BG, FONT_MONO},
};
use gpui::{
    div, relative, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_kit::component::{
    input::{Copy, Cut, InputEvent, Paste, Redo, SelectAll, Textarea, TextareaState, Undo},
    menu::ContextMenuExt,
};

mod graphemes;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(crate) enum MultilineInputEvent {
    ValueChanged(String),
}

/// Request-independent presentation around a retained Kit textarea. The owning pane projects
/// model values at request/mode boundaries; only actual edits are forwarded back to the model.
pub(crate) struct MultilineInput {
    input: Entity<TextareaState>,
    last_value: SharedString,
    label: &'static str,
    _subscription: Subscription,
}

impl MultilineInput {
    pub(crate) fn new(
        label: &'static str,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder)
                // Keep the existing Kit popup, including Undo/Redo, on every platform.
                .context_menu(false)
        });
        let subscription = cx.subscribe(&input, |this, input, event, cx| {
            if matches!(event, InputEvent::Change) {
                let value = input.read(cx).value();
                if value != this.last_value {
                    this.last_value = value.clone();
                    cx.emit(MultilineInputEvent::ValueChanged(value.to_string()));
                }
            }
        });
        Self {
            input,
            last_value: SharedString::default(),
            label,
            _subscription: subscription,
        }
    }

    pub(crate) fn value(&self) -> SharedString {
        self.last_value.clone()
    }

    /// Silent projection always clears undo/composition, even for equal-valued request tabs.
    pub(crate) fn project_content(
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

    #[cfg(test)]
    pub(crate) fn input_state(&self) -> Entity<TextareaState> {
        self.input.clone()
    }

    #[cfg(test)]
    pub(crate) fn set_content(
        &mut self,
        content: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |input, cx| {
            input.replace_all(content.into(), window, cx)
        });
    }
}

impl EventEmitter<MultilineInputEvent> for MultilineInput {}

impl Focusable for MultilineInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for MultilineInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle(cx);
        let menu_focus = focus.clone();
        graphemes::with_grapheme_boundaries(div().id("request-text-editor"), &self.input)
            .debug_selector(|| "request-text-editor".into())
            .key_context("RequestTextInput")
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(
                Textarea::new(&self.input)
                    .h(relative(1.))
                    .min_h_0()
                    .font_family(FONT_MONO)
                    .text_size(m::CODE)
                    .line_height(relative(1.8))
                    .bg(CODE_BG.resolve(cx))
                    .rounded(m::RADIUS)
                    .aria_label(self.label),
            )
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button == MouseButton::Right {
                    focus.focus(window, cx);
                }
            })
            .context_menu(move |menu, _, _| {
                edit_popup_menu(
                    menu,
                    menu_focus.clone(),
                    vec![
                        ("Undo", Box::new(Undo)),
                        ("Redo", Box::new(Redo)),
                        ("Cut", Box::new(Cut)),
                        ("Copy", Box::new(Copy)),
                        ("Paste", Box::new(Paste)),
                        ("Select All", Box::new(SelectAll)),
                    ],
                )
            })
    }
}
