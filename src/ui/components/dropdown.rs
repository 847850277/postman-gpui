use gpui::{
    anchored, canvas, deferred, div, prelude::FluentBuilder, px, rgb, AppContext, ClickEvent, Context, Element, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window
};

#[derive(Debug, Clone)]
pub enum DropdownEvent {
    SelectionChanged(String),
}

pub struct Dropdown {
    id: ElementId,
    focus_handle: FocusHandle,
    selected_value: String,
    options: Vec<String>,
    is_open: bool,
    placeholder: Option<String>,
    button_bounds: gpui::Bounds<gpui::Pixels>, // 添加按钮位置信息
}

impl Dropdown {
    pub fn new(id: impl Into<ElementId>, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            focus_handle: cx.focus_handle(),
            selected_value: String::new(),
            options: Vec::new(),
            is_open: false,
            placeholder: None,
            button_bounds: gpui::Bounds::default(), // 初始化
        }
    }

    pub fn with_options(mut self, options: Vec<String>) -> Self {
        if !options.is_empty() && self.selected_value.is_empty() {
            self.selected_value = options[0].clone();
        }
        self.options = options;
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn with_selected(mut self, selected: impl Into<String>) -> Self {
        self.selected_value = selected.into();
        self
    }

    pub fn selected_value(&self) -> &str {
        &self.selected_value
    }

    pub fn set_selected(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        let new_value = value.into();
        println!("🔽 Dropdown::set_selected - 设置值: {}", new_value);
        println!("🔽 Dropdown::set_selected - 当前值: {}", self.selected_value);
        println!("🔽 Dropdown::set_selected - 选项列表: {:?}", self.options);
        
        if self.selected_value != new_value && self.options.contains(&new_value) {
            println!("🔽 Dropdown::set_selected - 值有变化且有效，更新中...");
            self.selected_value = new_value.clone();
            cx.emit(DropdownEvent::SelectionChanged(new_value.clone()));
            cx.notify();
            println!("🔽 Dropdown::set_selected - 发送事件: DropdownEvent::SelectionChanged({})", new_value);
        } else {
            println!("🔽 Dropdown::set_selected - 值未变化或无效，跳过更新");
        }
    }

    fn toggle_dropdown(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        println!("🔽 Dropdown::toggle_dropdown - 切换下拉菜单状态: {} -> {}", self.is_open, !self.is_open);
        self.is_open = !self.is_open;
        cx.notify();
    }

    fn select_option(
        &mut self,
        option: String,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!("🔽 Dropdown::select_option - 选择选项: {}", option);
        println!("🔽 Dropdown::select_option - 之前的值: {}", self.selected_value);
        
        self.selected_value = option.clone();
        self.is_open = false;
        
        println!("🔽 Dropdown::select_option - 发送事件: DropdownEvent::SelectionChanged({})", option);
        cx.emit(DropdownEvent::SelectionChanged(option));
        cx.notify();
        
        println!("🔽 Dropdown::select_option - 完成，当前值: {}", self.selected_value);
    }

    fn render_dropdown_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let display_text = if self.selected_value.is_empty() {
            self.placeholder
                .as_ref()
                .unwrap_or(&"Select...".to_string())
                .clone()
        } else {
            self.selected_value.clone()
        };

        div()
            .id("dropdown-button")
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .min_w_32() // 设置最小宽度
            .px_3()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(if self.is_open {
                rgb(0x007bff)
            } else {
                rgb(0xcccccc)
            })
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(0x007bff)))
            .on_click(cx.listener(Self::toggle_dropdown))
            .child(
                div()
                    .flex_1()
                    .text_color(if self.selected_value.is_empty() {
                        rgb(0x999999)
                    } else {
                        rgb(0x333333)
                    })
                    .child(display_text),
            )
            .child(
                div()
                    .w_4()
                    .h_4()
                    .child(if self.is_open { "▲" } else { "▼" })
                    .text_color(rgb(0x666666)),
            )
            .child(
                // 使用 canvas 获取按钮的精确位置
                canvas(
                    move |bounds, _, _| {
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    fn render_dropdown_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.button_bounds;
        // 使用 deferred + anchored 确保菜单显示在最顶层
        // 关键：使用 deferred + anchored 组合确保在顶层渲染
        deferred(
            anchored()
                .snap_to_window_with_margin(px(8.)) // 确保不会超出窗口边界
                .child(
                    div()
                        .absolute()
                        .top(bounds.bottom() + px(2.)) // 在按钮下方 2px 处显示
                        .left(bounds.left())
                        .min_w(px(200.)) // 设置最小宽度 200px
                        .w(bounds.size.width) // 与按钮同宽
                        .bg(rgb(0xffffff))
                        .border_1()
                        .border_color(rgb(0xcccccc))
                        .rounded_md()
                        .shadow_lg()
                        .max_h_48()
                        .max_h(px(300.)) // 增加最大高度到 300px
                        .children(self.options.iter().enumerate().map(|(index, option)| {
                            let is_selected = option == &self.selected_value;
                            let option_clone = option.clone();
                            
                            div()
                                .id(("dropdown-option", index))
                                .w_full()
                                .px_3()
                                .py_2()
                                .cursor_pointer()
                                .bg(if is_selected {
                                    rgb(0xf0f8ff)
                                } else {
                                    rgb(0xffffff)
                                })
                                .hover(|style| {
                                    if !is_selected {
                                        style.bg(rgb(0xf5f5f5))
                                    } else {
                                        style
                                    }
                                })
                                .text_color(if is_selected {
                                    rgb(0x007bff)
                                } else {
                                    rgb(0x333333)
                                })
                                // 修复点击事件
                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _event, window, cx| {
                                    this.select_option(option_clone.clone(), &ClickEvent::default(), window, cx)
                                }))
                                .child(option.clone())
                                .when(is_selected, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .right_2()
                                            .child("✓")
                                            .text_color(rgb(0x007bff)),
                                    )
                                })
                        }))
                )
                // 点击外部关闭下拉菜单
                // .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                //     this.close_dropdown(&ClickEvent::default(), window, cx)
                // }))
        )
        .with_priority(1000) // 设置高渲染优先级，确保显示在最顶层
    }
}


impl EventEmitter<DropdownEvent> for Dropdown {}

impl Focusable for Dropdown {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}


impl Render for Dropdown {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
         div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .track_focus(&self.focus_handle)
            .child(self.render_dropdown_button(cx))
            .when(self.is_open, |this| {
                this.child(self.render_dropdown_menu(cx))
            })
    }
}