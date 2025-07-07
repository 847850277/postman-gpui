use gpui::{div, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, Styled, Subscription, Window};

use super::dropdown::{Dropdown, DropdownEvent};

#[derive(Debug, Clone)]
pub enum MethodSelectorEvent {
    MethodChanged(String),
}

pub struct MethodSelector {
    dropdown: Entity<Dropdown>,
    //_subscription: Subscription,
}

impl MethodSelector {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let dropdown = cx.new(|cx| {
            Dropdown::new("method-dropdown", cx)
                .with_options(vec![
                    "GET".to_string(),
                    "POST".to_string(),
                    "PUT".to_string(),
                    "DELETE".to_string(),
                    "PATCH".to_string(),
                    "HEAD".to_string(),
                    "OPTIONS".to_string(),
                ])
                .with_selected("GET")
                .with_placeholder("Select HTTP Method")
        });

        //let subscription = cx.subscribe(&dropdown, Self::on_dropdown_event);

        Self {
            dropdown,
            //_subscription: subscription,
        }
    }

    // pub fn selected_method(&self, cx: &dyn gpui::AppContext) -> String {
    //     self.dropdown.read(cx).selected_value().to_string()
    // }

    pub fn set_selected_method(&mut self, method: &str, cx: &mut Context<Self>) {
        self.dropdown.update(cx, |dropdown, cx| {
            dropdown.set_selected(method, cx);
        });
    }

    fn on_dropdown_event(
        &mut self,
        _dropdown: Entity<Dropdown>,
        event: &DropdownEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DropdownEvent::SelectionChanged(method) => {
                cx.emit(MethodSelectorEvent::MethodChanged(method.clone()));
            }
        }
    }
}

impl EventEmitter<MethodSelectorEvent> for MethodSelector {}

impl Render for MethodSelector {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_32() // 固定宽度
            .child(self.dropdown.clone())
    }
}