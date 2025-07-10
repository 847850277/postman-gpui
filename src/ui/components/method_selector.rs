use gpui::{
    div, AppContext, Context, Entity, EventEmitter, Focusable, IntoElement, ParentElement, Render,
    Styled, Subscription, Window,
};

use super::dropdown::{Dropdown, DropdownEvent};

#[derive(Debug, Clone)]
pub enum MethodSelectorEvent {
    MethodChanged(String),
}

pub struct MethodSelector {
    dropdown: Entity<Dropdown>,
    _subscription: Subscription,
}

impl MethodSelector {
    pub fn new(cx: &mut Context<Self>) -> Self {
        println!("🚀 MethodSelector::new - 创建方法选择器");

        let dropdown = cx.new(|cx| {
            let dropdown = Dropdown::new("method-dropdown", cx)
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
                .with_placeholder("Select HTTP Method");

            println!("🚀 MethodSelector::new - 下拉菜单创建完成，默认选中: GET");
            dropdown
        });

        let subscription = cx.subscribe(&dropdown, Self::on_dropdown_event);
        println!("🚀 MethodSelector::new - 订阅下拉菜单事件完成");

        Self {
            dropdown,
            _subscription: subscription,
        }
    }

    pub fn selected_method(&self, cx: &mut Context<Self>) -> String {
        let method = self.dropdown.read(cx).selected_value().to_string();
        println!(
            "📖 MethodSelector::selected_method - 当前选中方法: {}",
            method
        );
        println!("📖 调用栈: {:?}", std::backtrace::Backtrace::capture());
        method
    }

    pub fn set_selected_method(&mut self, method: &str, cx: &mut Context<Self>) {
        println!(
            "📝 MethodSelector::set_selected_method - 设置方法: {}",
            method
        );
        println!("📝 调用栈: {:?}", std::backtrace::Backtrace::capture());
        self.dropdown.update(cx, |dropdown, cx| {
            dropdown.set_selected(method, cx);
        });
        println!("📝 MethodSelector::set_selected_method - 方法设置完成");
    }

    fn on_dropdown_event(
        &mut self,
        _dropdown: Entity<Dropdown>,
        event: &DropdownEvent,
        cx: &mut Context<Self>,
    ) {
        println!(
            "📡 MethodSelector::on_dropdown_event - 接收到下拉菜单事件: {:?}",
            event
        );

        match event {
            DropdownEvent::SelectionChanged(method) => {
                println!(
                    "📡 MethodSelector::on_dropdown_event - 方法变更: {}",
                    method
                );
                println!("📡 MethodSelector::on_dropdown_event - 发送 MethodSelectorEvent::MethodChanged({})", method);
                cx.emit(MethodSelectorEvent::MethodChanged(method.clone()));
                println!("📡 MethodSelector::on_dropdown_event - 事件发送完成");
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
