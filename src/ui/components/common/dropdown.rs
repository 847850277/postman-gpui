use gpui::{
    actions, anchored, canvas, deferred, div, point, prelude::FluentBuilder, px, rgb, Anchor,
    ClickEvent, Context, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Role, StatefulInteractiveElement, Styled,
    Window,
};

use crate::{
    models::HttpMethod,
    ui::theme::{
        method_color, ACCENT_SOFT, FONT_HEADING, INFO_SOFT, LINE, OK_SOFT, PANEL, PANEL_ALT,
        SUBTEXT,
    },
};

#[derive(Debug, Clone)]
pub enum DropdownEvent {
    SelectionChanged(String),
}

actions!(
    dropdown,
    [
        ToggleDropdown,
        SelectPreviousOption,
        SelectNextOption,
        CloseDropdown,
    ]
);

pub fn setup_dropdown_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ToggleDropdown, Some("Dropdown")),
        KeyBinding::new("space", ToggleDropdown, Some("Dropdown")),
        KeyBinding::new("up", SelectPreviousOption, Some("Dropdown")),
        KeyBinding::new("left", SelectPreviousOption, Some("Dropdown")),
        KeyBinding::new("down", SelectNextOption, Some("Dropdown")),
        KeyBinding::new("right", SelectNextOption, Some("Dropdown")),
        KeyBinding::new("escape", CloseDropdown, Some("Dropdown")),
    ]
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
    fn method_palette(value: &str) -> (u32, u32) {
        let method: HttpMethod = value.into();
        let soft = match method {
            HttpMethod::GET => OK_SOFT,
            HttpMethod::POST => ACCENT_SOFT,
            HttpMethod::PUT => INFO_SOFT,
            HttpMethod::DELETE => 0x00fd_ecec,
            HttpMethod::PATCH => 0x00f1_ecfa,
            HttpMethod::HEAD | HttpMethod::OPTIONS => PANEL_ALT,
        };
        (method_color(method), soft)
    }

    fn method_border(value: &str) -> u32 {
        match HttpMethod::from(value) {
            HttpMethod::GET => 0x00a7_ddbd,
            HttpMethod::POST => 0x00f2_b89f,
            HttpMethod::PUT => 0x00a9_d3dd,
            HttpMethod::DELETE => 0x00e3_a7a7,
            HttpMethod::PATCH => 0x00c7_b9e5,
            HttpMethod::HEAD | HttpMethod::OPTIONS => LINE,
        }
    }

    pub fn new(id: impl Into<ElementId>, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
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

    pub fn set_selected(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        let new_value = value.into();
        if self.selected_value != new_value && self.options.contains(&new_value) {
            self.selected_value = new_value.clone();
            cx.emit(DropdownEvent::SelectionChanged(new_value));
            cx.notify();
        }
    }

    /// Projects a selected value without emitting a selection event.
    pub fn project_selected(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        let value = value.into();
        if self.selected_value != value && self.options.contains(&value) {
            self.selected_value = value;
            cx.notify();
        }
    }

    fn toggle_dropdown(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.is_open = !self.is_open;
        cx.notify();
    }

    fn toggle_with_keyboard(&mut self, _: &ToggleDropdown, _: &mut Window, cx: &mut Context<Self>) {
        self.is_open = !self.is_open;
        cx.notify();
    }

    fn select_relative(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.options.is_empty() {
            return;
        }
        let current = self
            .options
            .iter()
            .position(|option| option == &self.selected_value)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(self.options.len() as isize) as usize;
        let option = self.options[next].clone();
        self.selected_value.clone_from(&option);
        self.is_open = true;
        cx.emit(DropdownEvent::SelectionChanged(option));
        cx.notify();
    }

    fn select_previous(
        &mut self,
        _: &SelectPreviousOption,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_relative(-1, cx);
    }

    fn select_next(&mut self, _: &SelectNextOption, _: &mut Window, cx: &mut Context<Self>) {
        self.select_relative(1, cx);
    }

    fn close_with_keyboard(&mut self, _: &CloseDropdown, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_open {
            self.is_open = false;
            cx.notify();
        }
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

    fn render_dropdown_button(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let display_text = if self.selected_value.is_empty() {
            self.placeholder
                .as_ref()
                .unwrap_or(&"Select...".to_string())
                .clone()
        } else {
            self.selected_value.clone()
        };
        let (method_color, method_soft) = Self::method_palette(&self.selected_value);
        let method_border = Self::method_border(&self.selected_value);

        div()
            .id("dropdown-button")
            .debug_selector(|| "method-dropdown-button".into())
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h_full()
            .px(px(13.0))
            .bg(rgb(method_soft))
            .border_1()
            .border_color(if self.is_open || self.focus_handle.is_focused(window) {
                rgb(method_color)
            } else {
                rgb(method_border)
            })
            .rounded(px(9.0))
            .cursor_pointer()
            .font_family(FONT_HEADING)
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::BOLD)
            .hover(move |style| style.border_color(rgb(method_color)))
            .on_click(cx.listener(Self::toggle_dropdown))
            .child(
                div()
                    .debug_selector(|| "method-dropdown-selected-value".into())
                    .flex_1()
                    .text_color(if self.selected_value.is_empty() {
                        rgb(SUBTEXT)
                    } else {
                        rgb(method_color)
                    })
                    .child(display_text),
            )
            .child(
                div()
                    .size(px(15.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(FONT_HEADING)
                    .text_size(px(15.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(method_color))
                    .child(if self.is_open { "⌃" } else { "⌄" }),
            )
    }

    fn render_dropdown_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.button_bounds;
        let menu_width = if bounds.size.width > px(0.0) {
            bounds.size.width
        } else {
            px(120.0)
        };

        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(bounds.left(), bounds.bottom() + px(6.0)))
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .debug_selector(|| "method-dropdown-menu".into())
                        .w(menu_width)
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_1()
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(LINE))
                        .rounded(px(9.0))
                        .shadow_lg()
                        .max_h(px(320.0))
                        .overflow_hidden()
                        .children(self.options.iter().enumerate().map(|(index, option)| {
                            let is_selected = option == &self.selected_value;
                            let (option_color, option_soft) = Self::method_palette(option);
                            let option_clone = option.clone();
                            let debug_selector =
                                format!("method-option-{}", option.to_ascii_lowercase());

                            div()
                                .id(("dropdown-option", index))
                                .debug_selector(move || debug_selector.clone())
                                .w_full()
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .px(px(10.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .bg(if is_selected {
                                    rgb(option_soft)
                                } else {
                                    rgb(PANEL)
                                })
                                .hover(move |style| {
                                    if !is_selected {
                                        style.bg(rgb(option_soft))
                                    } else {
                                        style
                                    }
                                })
                                .text_color(rgb(option_color))
                                .font_family(FONT_HEADING)
                                .text_size(px(12.0))
                                .font_weight(if is_selected {
                                    gpui::FontWeight::BOLD
                                } else {
                                    gpui::FontWeight::SEMIBOLD
                                })
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
                                        div().flex_none().child("✓").text_color(rgb(option_color)),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dropdown = cx.entity().clone();

        div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .h_full()
            .track_focus(&self.focus_handle)
            .key_context("Dropdown")
            .role(Role::ComboBox)
            .aria_label("HTTP method")
            .aria_expanded(self.is_open)
            .on_action(cx.listener(Self::toggle_with_keyboard))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::close_with_keyboard))
            .child(self.render_dropdown_button(window, cx))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        dropdown.update(cx, |dropdown, _| dropdown.button_bounds = bounds)
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .when(self.is_open, |this| {
                this.child(self.render_dropdown_menu(cx))
            })
    }
}
