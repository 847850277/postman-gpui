use gpui::{
    anchored, canvas, deferred, div, prelude::FluentBuilder, px, rgb, ClickEvent, Context,
    ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};

use crate::ui::theme::{ACCENT, ACCENT_DARK, ACCENT_SOFT, FONT_HEADING, LINE, PANEL, SUBTEXT};

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
        tracing::info!("🔽 Dropdown::set_selected - 设置值: {new_value}");
        tracing::info!(
            "🔽 Dropdown::set_selected - 当前值: {}",
            self.selected_value
        );
        tracing::info!("🔽 Dropdown::set_selected - 选项列表: {:?}", self.options);

        if self.selected_value != new_value && self.options.contains(&new_value) {
            tracing::info!("🔽 Dropdown::set_selected - 值有变化且有效，更新中...");
            self.selected_value = new_value.clone();
            cx.emit(DropdownEvent::SelectionChanged(new_value.clone()));
            cx.notify();
            tracing::info!(
                "🔽 Dropdown::set_selected - 发送事件: DropdownEvent::SelectionChanged({})",
                new_value
            );
        } else {
            tracing::info!("🔽 Dropdown::set_selected - 值未变化或无效，跳过更新");
        }
    }

    fn toggle_dropdown(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            "🔽 Dropdown::toggle_dropdown - 切换下拉菜单状态: {} -> {}",
            self.is_open,
            !self.is_open
        );
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
        tracing::info!("🔽 Dropdown::select_option - 选择选项: {option}");
        tracing::info!(
            "🔽 Dropdown::select_option - 之前的值: {}",
            self.selected_value
        );
        self.selected_value = option.clone();
        self.is_open = false;
        tracing::info!(
            "🔽 Dropdown::select_option - 发送事件: DropdownEvent::SelectionChanged({})",
            option
        );
        cx.emit(DropdownEvent::SelectionChanged(option));
        cx.notify();
        tracing::info!(
            "🔽 Dropdown::select_option - 完成，当前值: {}",
            self.selected_value
        );
    }

    fn render_dropdown_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dropdown = cx.entity().clone();
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
            .h_full()
            .px_3()
            .bg(rgb(ACCENT_SOFT))
            .border_1()
            .border_color(if self.is_open {
                rgb(ACCENT)
            } else {
                rgb(ACCENT_SOFT)
            })
            .rounded_lg()
            .cursor_pointer()
            .font_family(FONT_HEADING)
            .text_size(px(15.0))
            .font_weight(gpui::FontWeight::BOLD)
            .hover(|style| style.border_color(rgb(ACCENT)))
            .on_click(cx.listener(Self::toggle_dropdown))
            .child(
                div()
                    .flex_1()
                    .text_color(if self.selected_value.is_empty() {
                        rgb(SUBTEXT)
                    } else {
                        rgb(ACCENT_DARK)
                    })
                    .child(display_text),
            )
            .child(
                div()
                    .w_4()
                    .h_4()
                    .child(if self.is_open { "▴" } else { "▾" })
                    .text_color(rgb(ACCENT_DARK)),
            )
            .child(
                // 使用 canvas 获取按钮的精确位置
                canvas(
                    move |bounds, _, cx| {
                        dropdown.update(cx, |dropdown, _| dropdown.button_bounds = bounds)
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
                        .w(px(120.0))
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(LINE))
                        .rounded_lg()
                        .shadow_lg()
                        .max_h(px(260.))
                        .overflow_hidden()
                        .children(self.options.iter().enumerate().map(|(index, option)| {
                            let is_selected = option == &self.selected_value;
                            let option_clone = option.clone();

                            div()
                                .id(("dropdown-option", index))
                                .w_full()
                                .h(px(34.0))
                                .flex()
                                .items_center()
                                .px_3()
                                .cursor_pointer()
                                .bg(if is_selected {
                                    rgb(0x00ff_f7ed)
                                } else {
                                    rgb(PANEL)
                                })
                                .hover(|style| {
                                    if !is_selected {
                                        style.bg(rgb(ACCENT_SOFT))
                                    } else {
                                        style
                                    }
                                })
                                .text_color(if is_selected {
                                    rgb(ACCENT_DARK)
                                } else {
                                    rgb(SUBTEXT)
                                })
                                .font_family(FONT_HEADING)
                                .text_size(px(13.0))
                                .font_weight(if is_selected {
                                    gpui::FontWeight::BOLD
                                } else {
                                    gpui::FontWeight::SEMIBOLD
                                })
                                // 修复点击事件
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _event, window, cx| {
                                        this.select_option(
                                            option_clone.clone(),
                                            &ClickEvent::default(),
                                            window,
                                            cx,
                                        )
                                    }),
                                )
                                .child(option.clone())
                                .when(is_selected, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .right_2()
                                            .child("✓")
                                            .text_color(rgb(ACCENT)),
                                    )
                                })
                        })),
                ), // 点击外部关闭下拉菜单
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
