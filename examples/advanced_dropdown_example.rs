use gpui::{
    anchored, canvas, deferred, div, prelude::FluentBuilder, px, rgb, size, App, AppContext,
    Application, Bounds, ClickEvent, Context, Element, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Window, WindowBounds, WindowOptions,
};

pub struct AdvancedDropdown {
    id: String,
    label: String,
    options: Vec<String>,
    selected_value: Option<String>,
    is_open: bool,
    button_bounds: gpui::Bounds<gpui::Pixels>,
}

impl AdvancedDropdown {
    pub fn new(id: impl Into<String>, label: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            options,
            selected_value: None,
            is_open: false,
            button_bounds: gpui::Bounds::default(),
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
        self.selected_value = Some(option);
        self.is_open = false;
        cx.notify();
    }

    fn close_dropdown(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.is_open = false;
        cx.notify();
    }

    fn render_dropdown_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let display_text = self
            .selected_value
            .as_ref()
            .cloned()
            .unwrap_or_else(|| format!("Select {}", self.label));

        div()
            .id("dropdown-button")
            .flex()
            .items_center()
            .justify_between()
            .w_48()
            .px_4()
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
            .on_click(cx.listener(Self::toggle_dropdown)) // 移除 into_element()
            .child(div().flex_1().child(display_text))
            .child(
                div()
                    .child(if self.is_open { "▲" } else { "▼" })
                    .text_color(rgb(0x666666)),
            )
            .child(
                // 获取按钮位置
                canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |dropdown, _| dropdown.button_bounds = bounds)
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
        deferred(
            anchored().snap_to_window_with_margin(px(8.)).child(
                div()
                    .absolute()
                    .top(bounds.bottom() + px(2.))
                    .left(bounds.left())
                    .w(bounds.size.width) // 与按钮同宽
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcccccc))
                    .rounded_md()
                    .shadow_lg()
                    .max_h_48()
                    //.overflow_y_auto() // 恢复滚动
                    .children(self.options.iter().map(|option| {
                        let option_clone = option.clone();
                        let is_selected = self.selected_value.as_ref() == Some(option);

                        div()
                            .id("dropdown-option")
                            .w_full()
                            .px_4()
                            .py_2()
                            .cursor_pointer()
                            .bg(if is_selected {
                                rgb(0xf0f8ff)
                            } else {
                                rgb(0xffffff)
                            })
                            .hover(|style| {
                                if is_selected {
                                    style.bg(rgb(0xe6f3ff)) // 已选中项的 hover 颜色
                                } else {
                                    style.bg(rgb(0xf5f5f5)) // 未选中项的 hover 颜色
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
                    })),
            ), //.on_mouse_down_out(cx.listener(Self::close_dropdown)) // 恢复点击外部关闭
        )
        .with_priority(200) // 高优先级渲染
    }
}

impl Render for AdvancedDropdown {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .child(self.render_dropdown_button(cx))
            .when(self.is_open, |this| {
                this.child(self.render_dropdown_menu(cx))
            })
    }
}

pub struct DropdownExample {
    dropdowns: Vec<gpui::Entity<AdvancedDropdown>>,
}

impl DropdownExample {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let dropdowns = vec![
            cx.new(|_| {
                AdvancedDropdown::new(
                    "countries",
                    "Country",
                    vec![
                        "USA".to_string(),
                        "Canada".to_string(),
                        "UK".to_string(),
                        "Germany".to_string(),
                    ],
                )
            }),
            cx.new(|_| {
                AdvancedDropdown::new(
                    "colors",
                    "Color",
                    vec![
                        "Red".to_string(),
                        "Green".to_string(),
                        "Blue".to_string(),
                        "Yellow".to_string(),
                    ],
                )
            }),
            cx.new(|_| {
                AdvancedDropdown::new(
                    "sizes",
                    "Size",
                    vec![
                        "Small".to_string(),
                        "Medium".to_string(),
                        "Large".to_string(),
                        "Extra Large".to_string(),
                    ],
                )
            }),
        ];

        Self { dropdowns }
    }
}

impl Render for DropdownExample {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0xf8f9fa))
            .size_full()
            .p_8()
            .gap_6()
            .child(
                div()
                    .child("Advanced Dropdown with Deferred + Anchored")
                    .text_size(px(28.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0x333333))
            )
            .child(
                div()
                    .child("These dropdowns use deferred + anchored to ensure they appear above all other content:")
                    .text_size(px(16.))
                    .text_color(rgb(0x666666))
            )
            .child(
                div()
                    .flex()
                    .gap_6()
                    .flex_wrap()
                    .children(self.dropdowns.clone())
            )
            .child(
                // 添加一些背景内容来演示层级效果
                div()
                    .mt_8()
                    .p_6()
                    .bg(rgb(0xffffff))
                    .rounded_lg()
                    .shadow_sm()
                    .border_1()
                    .border_color(rgb(0xe0e0e0))
                    .child(
                        div()
                            .child("Background Content")
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_size(px(18.))
                            .mb_4()
                    )
                    .child(
                        div()
                            .child("This content is behind the dropdowns. When you open a dropdown menu, it should appear above this content thanks to deferred + anchored rendering.")
                            .text_color(rgb(0x666666))
                            .line_height(px(24.)) // 修正 line_height，使用具体的像素值
                    )
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = gpui::Bounds::centered(None, size(px(900.), px(700.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| cx.new(DropdownExample::new))
            .expect("Failed to open window");
    });
}
