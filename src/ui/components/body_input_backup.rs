use gpui::{
    actions, div, px, rgb, Context, EventEmitter, FocusHandle, Focusable, 
    IntoElement, ParentElement, Render, Styled, Window, InteractiveElement,
    prelude::FluentBuilder, AppContext,
};

actions!(body_input, []);

#[derive(Debug, Clone, PartialEq)]
pub enum BodyType {
    Json,
    FormData,
    Raw,
}

#[derive(Debug, Clone)]
pub enum BodyInputEvent {
    ValueChanged(String),
    TypeChanged(BodyType),
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
        }
    }

    pub fn with_placeholder(self, _placeholder: impl Into<String>) -> Self {
        // 为了兼容性保留此方法
        self
    }

    pub fn get_current_type(&self) -> &BodyType {
        &self.current_type
    }

    pub fn set_type(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type != body_type {
            self.current_type = body_type.clone();
            cx.emit(BodyInputEvent::TypeChanged(body_type));
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

    pub fn get_json_content(&self) -> &str {
        &self.json_content
    }

    pub fn get_form_data_entries(&self) -> &Vec<FormDataEntry> {
        &self.form_data_entries
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content = content.into();
        
        match &self.current_type {
            BodyType::Json => {
                if self.json_content != new_content {
                    self.json_content = new_content.clone();
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::Raw => {
                if self.raw_content != new_content {
                    self.raw_content = new_content.clone();
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::FormData => {
                // FormData 不支持直接设置内容
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

    pub fn update_form_data_entry(&mut self, index: usize, key: String, value: String, enabled: bool, cx: &mut Context<Self>) {
        if let Some(entry) = self.form_data_entries.get_mut(index) {
            entry.key = key;
            entry.value = value;
            entry.enabled = enabled;
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

    pub fn is_empty(&self) -> bool {
        match &self.current_type {
            BodyType::Json => self.json_content.is_empty(),
            BodyType::Raw => self.raw_content.is_empty(),
            BodyType::FormData => {
                self.form_data_entries.iter().all(|entry| entry.key.is_empty() && entry.value.is_empty())
            }
        }
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .when(self.current_type == BodyType::Json, |div| {
                        div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                    })
                    .when(self.current_type != BodyType::Json, |div| {
                        div.bg(rgb(0x00f8_f9fa)).text_color(rgb(0x006c_757d))
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
                    .when(self.current_type == BodyType::FormData, |div| {
                        div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                    })
                    .when(self.current_type != BodyType::FormData, |div| {
                        div.bg(rgb(0x00f8_f9fa)).text_color(rgb(0x006c_757d))
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
                    .when(self.current_type == BodyType::Raw, |div| {
                        div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                    })
                    .when(self.current_type != BodyType::Raw, |div| {
                        div.bg(rgb(0x00f8_f9fa)).text_color(rgb(0x006c_757d))
                            .hover(|style| style.bg(rgb(0x00e9_ecef)))
                    })
                    .child("Raw")
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.set_type(BodyType::Raw, cx);
                        }),
                    ),
            )
    }

    fn render_json_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
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
                            .child(if self.json_content.is_empty() {
                                "Enter JSON body here..."
                            } else {
                                &self.json_content
                            })
                            .when(self.json_content.is_empty(), |div| {
                                div.text_color(rgb(0x006c_757d))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x0017_a2b8))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0013_8496)))
                            .child("Format")
                            .text_size(px(12.0)),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x0028_a745))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0021_8838)))
                            .child("Validate")
                            .text_size(px(12.0)),
                    ),
            )
    }

    fn render_form_data_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(
                        self.form_data_entries
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| {
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w_4()
                                            .h_8()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .w_4()
                                                    .h_4()
                                                    .border_1()
                                                    .border_color(rgb(0x00cc_cccc))
                                                    .when(entry.enabled, |div| {
                                                        div.bg(rgb(0x0000_7acc))
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(0x00ff_ffff))
                                            .border_1()
                                            .border_color(rgb(0x00cc_cccc))
                                            .child(if entry.key.is_empty() { "Key" } else { &entry.key }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(0x00ff_ffff))
                                            .border_1()
                                            .border_color(rgb(0x00cc_cccc))
                                            .child(if entry.value.is_empty() { "Value" } else { &entry.value }),
                                    )
                                    .child(
                                        div()
                                            .w_16()
                                            .px_2()
                                            .py_1()
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
                            })
                            .collect::<Vec<_>>(),
                    ),
            )
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
    }

    fn render_raw_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(if self.raw_content.is_empty() {
                        "Enter raw body here..."
                    } else {
                        &self.raw_content
                    })
                    .when(self.raw_content.is_empty(), |div| {
                        div.text_color(rgb(0x006c_757d))
                    }),
            )
    }
}

impl EventEmitter<BodyInputEvent> for BodyInput {}
impl Focusable for BodyInput {
    fn focus_handle(&self, _cx: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BodyInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.render_tabs(cx))
            .child(match &self.current_type {
                BodyType::Json => self.render_json_editor(cx).into_any_element(),
                BodyType::FormData => self.render_form_data_editor(cx).into_any_element(),
                BodyType::Raw => self.render_raw_editor(cx).into_any_element(),
            })
    }
}

pub fn setup_body_input_key_bindings() -> Vec<gpui::KeyBinding> {
    vec![]
}
