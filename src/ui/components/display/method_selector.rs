use gpui::{
    div, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, Styled,
    Subscription, Window,
};

use crate::models::HttpMethod;
use crate::ui::components::common::dropdown::{Dropdown, DropdownEvent};
use crate::ui::theme::{ACCENT_SOFT, FONT_HEADING};

#[derive(Debug, Clone)]
pub enum MethodSelectorEvent {
    MethodChanged(HttpMethod),
}

pub struct MethodSelector {
    dropdown: Entity<Dropdown>,
    _subscription: Subscription,
}

impl MethodSelector {
    pub fn new(cx: &mut Context<Self>) -> Self {
        tracing::info!("🚀 MethodSelector::new - 创建方法选择器");
        let dropdown = cx.new(|cx| {
            let dropdown = Dropdown::new("method-dropdown", cx)
                .with_options(
                    HttpMethod::all()
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<String>>(),
                )
                .with_selected("GET")
                .with_placeholder("Select HTTP Method");

            tracing::info!("🚀 MethodSelector::new - 下拉菜单创建完成，默认选中: GET");
            dropdown
        });

        let subscription = cx.subscribe(&dropdown, Self::on_dropdown_event);
        tracing::info!("🚀 MethodSelector::new - 订阅下拉菜单事件完成");

        Self {
            dropdown,
            _subscription: subscription,
        }
    }

    /// Projects the ViewModel method into the dropdown without emitting a user event.
    pub fn project_method(&mut self, method: HttpMethod, cx: &mut Context<Self>) {
        self.dropdown.update(cx, |dropdown, cx| {
            dropdown.project_selected(method.to_string(), cx);
        });
    }

    fn on_dropdown_event(
        &mut self,
        _dropdown: Entity<Dropdown>,
        event: &DropdownEvent,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("📡 MethodSelector::on_dropdown_event - 接收到下拉菜单事件: {event:?}");

        match event {
            DropdownEvent::SelectionChanged(method_str) => {
                tracing::info!("📡 MethodSelector::on_dropdown_event - 方法变更: {method_str}");
                let method: HttpMethod = method_str.as_str().into();
                tracing::info!("📡 MethodSelector::on_dropdown_event - 发送 MethodSelectorEvent::MethodChanged({method})");
                cx.emit(MethodSelectorEvent::MethodChanged(method));
                tracing::info!("📡 MethodSelector::on_dropdown_event - 事件发送完成");
            }
        }
    }
}

impl EventEmitter<MethodSelectorEvent> for MethodSelector {}

impl Render for MethodSelector {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(gpui::px(120.0))
            .h_full()
            .flex_none()
            .bg(gpui::rgb(ACCENT_SOFT))
            .rounded_lg()
            .font_family(FONT_HEADING)
            .child(self.dropdown.clone())
    }
}
