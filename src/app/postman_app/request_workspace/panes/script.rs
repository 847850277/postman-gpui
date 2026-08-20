use crate::{
    app::WorkspaceViewModel,
    ui::{
        components::body_input::{BodyInput, BodyInputEvent, BodyType},
        theme::{CODE_BG, CODE_TEXT, FONT_UI, MUTED},
    },
};
use gpui::{
    div, px, rgb, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::postman_app::request_workspace) enum ScriptPaneKind {
    PreRequest,
    Tests,
}

/// One script editor entity is instantiated for pre-request code and one for response tests.
pub(in crate::app::postman_app::request_workspace) struct ScriptPane {
    view_model: Entity<WorkspaceViewModel>,
    kind: ScriptPaneKind,
    input: Entity<BodyInput>,
    _subscriptions: Vec<Subscription>,
}

impl ScriptPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        kind: ScriptPaneKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = match kind {
            ScriptPaneKind::PreRequest => "Pre-request script",
            ScriptPaneKind::Tests => "Response tests",
        };
        let input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder(placeholder)
                .with_type_tabs(false)
        });
        let subscriptions = vec![cx.subscribe(&input, Self::on_input_event)];
        let mut pane = Self {
            view_model,
            kind,
            input,
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
    }

    fn update_view_model<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspaceViewModel) -> R,
    ) -> R {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = update(view_model);
            cx.notify();
            result
        });
        cx.notify();
        result
    }

    fn on_input_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let BodyInputEvent::ValueChanged(script) = event {
            match self.kind {
                ScriptPaneKind::PreRequest => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_pre_request_script(script)
                    });
                }
                ScriptPaneKind::Tests => {
                    self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
                }
            }
        }
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let content = {
            let view_model = self.view_model.read(cx);
            match self.kind {
                ScriptPaneKind::PreRequest => view_model.pre_request_script(),
                ScriptPaneKind::Tests => view_model.tests_script(),
            }
            .to_string()
        };
        self.input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Raw, cx);
            input.project_content(content, cx);
        });
        cx.notify();
    }

    fn render_script_editor(
        &self,
        title: &'static str,
        hint: &'static str,
        input: Entity<BodyInput>,
        selector: &'static str,
    ) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(rgb(CODE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family(FONT_UI)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(CODE_TEXT))
                            .child(title),
                    )
                    .child(div().text_size(px(11.0)).text_color(rgb(MUTED)).child(hint)),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .flex_1()
                    .min_h_0()
                    .child(input),
            )
            .into_any_element()
    }
}

impl Render for ScriptPane {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (title, hint, selector) = match self.kind {
            ScriptPaneKind::PreRequest => (
                "Pre-request script",
                "Saved with this request tab.",
                "script-editor",
            ),
            ScriptPaneKind::Tests => (
                "Response tests",
                "Saved with this request tab for the test runner.",
                "tests-editor",
            ),
        };
        self.render_script_editor(title, hint, self.input.clone(), selector)
    }
}
