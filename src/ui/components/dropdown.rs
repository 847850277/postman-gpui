use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, ClickEvent, Context, Element, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window
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
        if self.selected_value != new_value && self.options.contains(&new_value) {
            self.selected_value = new_value.clone();
            cx.emit(DropdownEvent::SelectionChanged(new_value));
            cx.notify();
        }
    }

    fn toggle_dropdown(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
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
        self.selected_value = option.clone();
        self.is_open = false;
        cx.emit(DropdownEvent::SelectionChanged(option));
        cx.notify();
    }

    fn close_dropdown(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open {
            self.is_open = false;
            cx.notify();
        }
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
    }

    fn render_dropdown_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("dropdown-menu")
            .absolute()
            .top_full()
            .left_0()
            .right_0()
            //.z_index(1000)
            .mt_1()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xcccccc))
            .rounded_md()
            .shadow_lg()
            .max_h_48()
            //.overflow_y_auto()
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
                    .on_click(cx.listener(move |this, event, window, cx| {
                        this.select_option(option_clone.clone(), event, window, cx)
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
                    .child(
                        // 背景遮罩，点击关闭下拉菜单
                        div()
                            .absolute()
                            .inset_0()
                            .cursor_pointer()
                            //.on_click(cx.listener(Self::close_dropdown)),
                    )
            })
    }
}