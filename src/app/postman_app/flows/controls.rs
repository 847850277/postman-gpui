use super::{model::validate_name, FlowsView};
use crate::ui::theme::{metrics as m, ERROR, MUTED, SUBTEXT};
use gpui::{
    div, prelude::FluentBuilder, Context, Focusable, InteractiveElement, ParentElement, Styled,
    Window,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::Confirm,
    input::Input,
    Disableable, WindowExt,
};

impl FlowsView {
    pub(super) fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        self.create_name
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.create_open = true;
        self.create_attempted = false;
        let owner = cx.weak_entity();
        let name = self.create_name.clone();
        let restore = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        window.open_dialog(cx, move |dialog, window, cx| {
            let error = validate_name(&name.read(cx).value()).err();
            let attempted = owner
                .read_with(cx, |this, _| this.create_attempted)
                .unwrap_or(false);
            let submit = owner.clone();
            let close = owner.clone();
            let restore = restore.clone();
            dialog
                .title("Create a flow")
                .w((window.rem_size() * 30.)
                    .min(window.viewport_size().width - window.rem_size() * 2.))
                .child(
                    div()
                        .debug_selector(|| "flow-create-dialog".into())
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_size(m::BODY)
                                .text_color(SUBTEXT.resolve(cx))
                                .child("Give your sequence a name, then add HTTP steps."),
                        )
                        .child(div().text_size(m::LABEL).child("Flow name"))
                        .child(
                            div().debug_selector(|| "flow-create-name".into()).child(
                                Input::new(&name)
                                    .id("flow-create-name")
                                    .aria_label("New flow name")
                                    .w_full(),
                            ),
                        )
                        .when(
                            (attempted || !name.read(cx).value().is_empty()) && error.is_some(),
                            |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "flow-create-error".into())
                                        .text_size(m::LABEL)
                                        .text_color(ERROR.resolve(cx))
                                        .child(error.unwrap_or_default()),
                                )
                            },
                        )
                        .child(
                            div()
                                .text_size(m::LABEL)
                                .text_color(MUTED.resolve(cx))
                                .child(
                                "Drafts last for this session. File saving is not available yet.",
                            ),
                        ),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(Button::new("flow-create-cancel").label("Cancel").on_click(
                            |_, w, c| {
                                w.close_dialog(c);
                            },
                        ))
                        .child(
                            Button::new("flow-create-submit")
                                .primary()
                                .label("Create flow")
                                .disabled(error.is_some())
                                .on_click(|_, w, c| {
                                    w.dispatch_action(Box::new(Confirm { secondary: false }), c)
                                }),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    submit
                        .update(cx, |this, cx| {
                            this.create_attempted = true;
                            let name = this.create_name.read(cx).value();
                            if this.library.create(&name).is_err() {
                                cx.notify();
                                return false;
                            }
                            this.conceal_inputs(cx);
                            this.sync_fields(window, cx);
                            this.sync_picker(window, cx);
                            this.details = false;
                            cx.notify();
                            true
                        })
                        .unwrap_or(true)
                })
                .on_close(move |_, window, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.create_open = false;
                        cx.notify();
                    });
                    restore.focus(window, cx);
                })
        });
        self.create_name.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn open_add_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library.active().is_some() && !window.has_active_dialog(cx) {
            cx.emit(super::FlowsEvent::AddStep);
        }
    }
}
