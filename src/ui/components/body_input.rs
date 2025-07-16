use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, App, Context, CursorStyle, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, KeyDownEvent,
    ParentElement, Render, Styled, Window,
};

actions!(
    body_input,
    [Backspace, Delete, Enter, Escape, Tab, ShiftTab,]
);

#[derive(Debug, Clone, PartialEq)]
pub enum BodyType {
    Json,
    FormData,
    Raw,
}

#[derive(Debug, Clone)]
pub enum BodyInputEvent {
    ValueChanged(String),
}

#[derive(Debug, Clone)]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

pub struct BodyInput {
    focus_handle: FocusHandle,
    current_type: BodyType,
    json_content: String,
    form_data_entries: Vec<FormDataEntry>,
    raw_content: String,
    editing_key_index: Option<usize>,
    editing_value_index: Option<usize>,
    temp_key_value: String,
    temp_value_value: String,
}

impl EventEmitter<BodyInputEvent> for BodyInput {}

impl Focusable for BodyInput {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl BodyInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            current_type: BodyType::Json,
            json_content: String::new(),
            form_data_entries: vec![FormDataEntry {
                key: String::new(),
                value: String::new(),
                enabled: true,
            }],
            raw_content: String::new(),
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
        }
    }

    pub fn with_placeholder(self, _placeholder: &str) -> Self {
        self
    }

    pub fn get_current_type(&self) -> &BodyType {
        &self.current_type
    }

    pub fn get_json_content(&self) -> &str {
        &self.json_content
    }

    pub fn get_form_data_entries(&self) -> &[FormDataEntry] {
        &self.form_data_entries
    }

    pub fn is_empty(&self) -> bool {
        match &self.current_type {
            BodyType::Json => self.json_content.is_empty(),
            BodyType::Raw => self.raw_content.is_empty(),
            BodyType::FormData => self
                .form_data_entries
                .iter()
                .all(|entry| entry.key.is_empty() && entry.value.is_empty()),
        }
    }

    pub fn set_type(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type != body_type {
            self.current_type = body_type;
            let content = match &self.current_type {
                BodyType::Json => self.json_content.clone(),
                BodyType::Raw => self.raw_content.clone(),
                BodyType::FormData => self.get_form_data_as_string(),
            };
            cx.emit(BodyInputEvent::ValueChanged(content));
            cx.notify();
        }
    }

    pub fn get_content(&self) -> String {
        match &self.current_type {
            BodyType::Json => self.json_content.clone(),
            BodyType::Raw => self.raw_content.clone(),
            BodyType::FormData => self.get_form_data_as_string(),
        }
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content = content.into();

        match &self.current_type {
            BodyType::Json => {
                if self.json_content != new_content {
                    self.json_content.clone_from(&new_content);
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::Raw => {
                if self.raw_content != new_content {
                    self.raw_content.clone_from(&new_content);
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::FormData => {
                // FormData does not support direct content setting
            }
        }
    }

    pub fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.form_data_entries.push(FormDataEntry {
            key: String::new(),
            value: String::new(),
            enabled: true,
        });
        cx.notify();
    }

    pub fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.form_data_entries.len() {
            self.form_data_entries.remove(index);
            if self.form_data_entries.is_empty() {
                self.form_data_entries.push(FormDataEntry {
                    key: String::new(),
                    value: String::new(),
                    enabled: true,
                });
            }
            cx.notify();
        }
    }

    pub fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.form_data_entries.get_mut(index) {
            entry.enabled = !entry.enabled;
            cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            cx.notify();
        }
    }

    pub fn get_form_data_as_string(&self) -> String {
        self.form_data_entries
            .iter()
            .filter(|entry| entry.enabled && !entry.key.is_empty())
            .map(|entry| format!("{}={}", entry.key, entry.value))
            .collect::<Vec<_>>()
            .join("&")
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        match &self.current_type {
            BodyType::Json => {
                self.json_content.clear();
            }
            BodyType::Raw => {
                self.raw_content.clear();
            }
            BodyType::FormData => {
                self.form_data_entries = vec![FormDataEntry {
                    key: String::new(),
                    value: String::new(),
                    enabled: true,
                }];
            }
        }
        cx.emit(BodyInputEvent::ValueChanged(String::new()));
        cx.notify();
    }

    pub fn start_editing_key(&mut self, index: usize, cx: &mut Context<Self>) {
        // 首先完成任何现有的编辑
        if self.editing_value_index.is_some() {
            self.finish_value_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            self.editing_key_index = Some(index);
            self.editing_value_index = None;
            self.temp_key_value = entry.key.clone();
            cx.notify();
        }
    }

    pub fn start_editing_value(&mut self, index: usize, cx: &mut Context<Self>) {
        // 首先完成任何现有的编辑
        if self.editing_key_index.is_some() {
            self.finish_key_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            self.editing_value_index = Some(index);
            self.editing_key_index = None;
            self.temp_value_value = entry.value.clone();
            cx.notify();
        }
    }

    pub fn finish_editing(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_key_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.key = self.temp_key_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
        }
        if let Some(index) = self.editing_value_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.value = self.temp_value_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
        }
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub fn finish_key_editing_only(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_key_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.key = self.temp_key_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
            self.editing_key_index = None;
            self.temp_key_value.clear();
        }
        cx.notify();
    }

    pub fn finish_value_editing_only(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_value_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.value = self.temp_value_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
            self.editing_value_index = None;
            self.temp_value_value.clear();
        }
        cx.notify();
    }

    pub fn cancel_editing(&mut self, cx: &mut Context<Self>) {
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub fn update_temp_key(&mut self, value: String, cx: &mut Context<Self>) {
        self.temp_key_value = value;
        cx.notify();
    }

    pub fn update_temp_value(&mut self, value: String, cx: &mut Context<Self>) {
        self.temp_value_value = value;
        cx.notify();
    }

    // Action handlers for keyboard shortcuts
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(_index) = self.editing_key_index {
            self.temp_key_value.pop();
            cx.notify();
        } else if let Some(_index) = self.editing_value_index {
            self.temp_value_value.pop();
            cx.notify();
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        // 对于简单的文本输入，delete 和 backspace 行为相同
        self.backspace(&Backspace, window, cx);
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.finish_editing(cx);
        }
    }

    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.cancel_editing(cx);
        }
    }

    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        // Tab 键在 FormData 条目之间导航
        if let Some(index) = self.editing_key_index {
            // 从 key 切换到 value - start_editing_value 会自动完成 key 编辑
            self.start_editing_value(index, cx);
        } else if let Some(index) = self.editing_value_index {
            // 从 value 切换到下一行的 key，或者添加新行
            if index + 1 < self.form_data_entries.len() {
                self.start_editing_key(index + 1, cx);
            } else {
                self.add_form_data_entry(cx);
                self.start_editing_key(self.form_data_entries.len() - 1, cx);
            }
        }
    }

    fn shift_tab(&mut self, _: &ShiftTab, _: &mut Window, cx: &mut Context<Self>) {
        // Shift+Tab 键反向导航
        if let Some(index) = self.editing_value_index {
            // 从 value 切换到 key - start_editing_key 会自动完成 value 编辑
            self.start_editing_key(index, cx);
        } else if let Some(index) = self.editing_key_index {
            // 从 key 切换到上一行的 value
            if index > 0 {
                self.start_editing_value(index - 1, cx);
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // 只在编辑模式下处理字符输入
        if self.editing_key_index.is_none() && self.editing_value_index.is_none() {
            return;
        }

        // 处理普通字符输入
        if let Some(key_char) = &event.keystroke.key_char {
            // 过滤掉特殊键和控制字符
            if key_char.len() == 1 && !key_char.chars().any(|c| c.is_control()) {
                if self.editing_key_index.is_some() {
                    self.temp_key_value.push_str(key_char);
                    cx.notify();
                } else if self.editing_value_index.is_some() {
                    self.temp_value_value.push_str(key_char);
                    cx.notify();
                }
            }
        }
    }
}

impl Render for BodyInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_type = self.current_type.clone();
        let json_content = self.json_content.clone();
        let raw_content = self.raw_content.clone();
        let form_data_entries = self.form_data_entries.clone();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .h_full()
            // Tab headers
            .child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(0x00dd_dddd))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::Json, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::Json, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("JSON")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::Json, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::FormData, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::FormData, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("Form Data")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::FormData, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::Raw, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::Raw, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("Raw")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::Raw, cx);
                                }),
                            ),
                    ),
            )
            // Content area
            .child(match current_type {
                BodyType::Json => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .w_full()
                            .h_64()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x00ff_ffff))
                            .border_1()
                            .border_color(rgb(0x00cc_cccc))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_family("monospace")
                                    .child(if json_content.is_empty() {
                                        "Enter JSON body here...".to_string()
                                    } else {
                                        json_content
                                    })
                                    .when(self.json_content.is_empty(), |div| {
                                        div.text_color(rgb(0x006c_757d))
                                    }),
                            ),
                    )
                    .into_any_element(),
                BodyType::FormData => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .track_focus(&self.focus_handle(cx))
                    .on_action(cx.listener(Self::backspace))
                    .on_action(cx.listener(Self::delete))
                    .on_action(cx.listener(Self::enter))
                    .on_action(cx.listener(Self::escape))
                    .on_action(cx.listener(Self::tab))
                    .on_action(cx.listener(Self::shift_tab))
                    .on_key_down(cx.listener(Self::on_key_down))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .p_2()
                            .bg(rgb(0x00f8_f9fa))
                            .border_1()
                            .border_color(rgb(0x00de_e2e6))
                            .child(
                                div()
                                    .w_4()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("✓"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Key"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Value"),
                            )
                            .child(
                                div()
                                    .w_16()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Action"),
                            ),
                    )
                    .child(div().flex().flex_col().gap_2().children(
                        form_data_entries.iter().enumerate().map(|(index, entry)| {
                            let entry_key = entry.key.clone();
                            let entry_value = entry.value.clone();
                            let entry_enabled = entry.enabled;

                            div()
                                .flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    // Checkbox
                                    div()
                                        .w_4()
                                        .h_4()
                                        .border_1()
                                        .border_color(rgb(0x00cc_cccc))
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .when(entry_enabled, |style| {
                                            style.bg(rgb(0x0000_7acc)).child(
                                                div().w_2().h_2().bg(rgb(0x00ff_ffff)).m_auto(),
                                            )
                                        })
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.toggle_form_data_entry(index, cx);
                                            }),
                                        ),
                                )
                                .child(
                                    // Key input - 可点击编辑
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00ff_ffff))
                                        .border_1()
                                        .border_color(if self.editing_key_index == Some(index) {
                                            rgb(0x0000_7acc)
                                        } else {
                                            rgb(0x00cc_cccc)
                                        })
                                        .rounded_md()
                                        .text_size(px(14.0))
                                        .cursor(CursorStyle::IBeam)
                                        .when(self.editing_key_index == Some(index), |div| {
                                            div.child(self.temp_key_value.clone())
                                        })
                                        .when(self.editing_key_index != Some(index), |div| {
                                            div.when(entry_key.is_empty(), |div| {
                                                div.text_color(rgb(0x006c_757d))
                                                    .child("Enter key...")
                                            })
                                            .when(!entry_key.is_empty(), |div| {
                                                div.text_color(rgb(0x0021_2529))
                                                    .child(entry_key.clone())
                                            })
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.start_editing_key(index, cx);
                                                }),
                                            )
                                        }),
                                )
                                .child(
                                    // Value input - 可点击编辑
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00ff_ffff))
                                        .border_1()
                                        .border_color(if self.editing_value_index == Some(index) {
                                            rgb(0x0000_7acc)
                                        } else {
                                            rgb(0x00cc_cccc)
                                        })
                                        .rounded_md()
                                        .text_size(px(14.0))
                                        .cursor(CursorStyle::IBeam)
                                        .when(self.editing_value_index == Some(index), |div| {
                                            div.child(self.temp_value_value.clone())
                                        })
                                        .when(self.editing_value_index != Some(index), |div| {
                                            div.when(entry_value.is_empty(), |div| {
                                                div.text_color(rgb(0x006c_757d))
                                                    .child("Enter value...")
                                            })
                                            .when(!entry_value.is_empty(), |div| {
                                                div.text_color(rgb(0x0021_2529))
                                                    .child(entry_value.clone())
                                            })
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.start_editing_value(index, cx);
                                                }),
                                            )
                                        }),
                                )
                                .child(
                                    // Delete button
                                    div()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00dc_3545))
                                        .text_color(rgb(0x00ff_ffff))
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x00c8_2333)))
                                        .child("Delete")
                                        .text_size(px(12.0))
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.remove_form_data_entry(index, cx);
                                            }),
                                        ),
                                )
                        }),
                    ))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x0028_a745))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0021_8838)))
                            .child("Add Row")
                            .text_size(px(14.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.add_form_data_entry(cx);
                                }),
                            ),
                    )
                    .into_any_element(),
                BodyType::Raw => div()
                    .w_full()
                    .h_64()
                    .px_3()
                    .py_2()
                    .bg(rgb(0x00ff_ffff))
                    .border_1()
                    .border_color(rgb(0x00cc_cccc))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_family("monospace")
                            .child(if raw_content.is_empty() {
                                "Enter raw body here...".to_string()
                            } else {
                                raw_content
                            })
                            .when(self.raw_content.is_empty(), |div| {
                                div.text_color(rgb(0x006c_757d))
                            }),
                    )
                    .into_any_element(),
            })
    }
}

pub fn setup_body_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("enter", Enter, None),
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("tab", Tab, None),
        KeyBinding::new("shift-tab", ShiftTab, None),
    ]
}
