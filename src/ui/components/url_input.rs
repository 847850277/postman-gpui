use gpui::{
    div, rgb, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};

#[derive(Debug, Clone)]
pub enum UrlInputEvent {
    UrlChanged(String),
    SubmitRequested,
}

pub struct UrlInput {
    url: String,
    placeholder: String,
    focus_handle: FocusHandle,
    is_editing: bool,
    cursor_position: usize,
    selection_start: Option<usize>,
    selection_end: Option<usize>,
}

impl UrlInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            url: String::new(),
            placeholder: "Enter request URL".to_string(),
            focus_handle: cx.focus_handle(),
            is_editing: false,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub fn set_url(&mut self, url: impl Into<String>, cx: &mut Context<Self>) {
        let new_url = url.into();
        if self.url != new_url {
            self.url = new_url.clone();
            self.cursor_position = self.url.len().min(self.cursor_position);
            self.clear_selection();
            cx.emit(UrlInputEvent::UrlChanged(new_url));
            cx.notify();
        }
    }

    pub fn submit_url(&mut self, cx: &mut Context<Self>) {
        println!("Submitted URL: {}", self.url);
        cx.emit(UrlInputEvent::SubmitRequested);
    }

    // 清除选择
    fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    // 获取选中的文本范围
    fn get_selection_range(&self) -> Option<(usize, usize)> {
        match (self.selection_start, self.selection_end) {
            (Some(start), Some(end)) => {
                let start = start.min(self.url.len());
                let end = end.min(self.url.len());
                Some((start.min(end), start.max(end)))
            }
            _ => None,
        }
    }

    // 删除选中的文本
    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            if start != end {
                self.url.drain(start..end);
                self.cursor_position = start;
                self.clear_selection();
                return true;
            }
        }
        false
    }

    // 插入字符
    fn insert_char(&mut self, ch: char, cx: &mut Context<Self>) {
        // 如果有选中的文本，先删除
        self.delete_selection();

        // 插入字符
        if self.cursor_position <= self.url.len() {
            self.url.insert(self.cursor_position, ch);
            self.cursor_position += 1;
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
        }
    }

    // 插入文本（用于粘贴等）
    fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        // 如果有选中的文本，先删除
        self.delete_selection();

        // 插入文本
        if self.cursor_position <= self.url.len() {
            self.url.insert_str(self.cursor_position, text);
            self.cursor_position += text.len();
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
        }
    }

    // 删除字符（退格）
    fn backspace(&mut self, cx: &mut Context<Self>) {
        // 如果有选中的文本，删除选中的文本
        if self.delete_selection() {
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
            return;
        }

        // 否则删除光标前的字符
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.url.remove(self.cursor_position);
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
        }
    }

    // 删除字符（Delete键）
    fn delete(&mut self, cx: &mut Context<Self>) {
        // 如果有选中的文本，删除选中的文本
        if self.delete_selection() {
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
            return;
        }

        // 否则删除光标后的字符
        if self.cursor_position < self.url.len() {
            self.url.remove(self.cursor_position);
            cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
            cx.notify();
        }
    }

    // 移动光标
    fn move_cursor_left(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        if self.cursor_position > 0 {
            if extend_selection {
                if self.selection_start.is_none() {
                    self.selection_start = Some(self.cursor_position);
                }
                self.cursor_position -= 1;
                self.selection_end = Some(self.cursor_position);
            } else {
                self.cursor_position -= 1;
                self.clear_selection();
            }
            cx.notify();
        }
    }

    fn move_cursor_right(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        if self.cursor_position < self.url.len() {
            if extend_selection {
                if self.selection_start.is_none() {
                    self.selection_start = Some(self.cursor_position);
                }
                self.cursor_position += 1;
                self.selection_end = Some(self.cursor_position);
            } else {
                self.cursor_position += 1;
                self.clear_selection();
            }
            cx.notify();
        }
    }

    fn move_cursor_to_start(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        if extend_selection {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor_position);
            }
            self.cursor_position = 0;
            self.selection_end = Some(self.cursor_position);
        } else {
            self.cursor_position = 0;
            self.clear_selection();
        }
        cx.notify();
    }

    fn move_cursor_to_end(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        if extend_selection {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor_position);
            }
            self.cursor_position = self.url.len();
            self.selection_end = Some(self.cursor_position);
        } else {
            self.cursor_position = self.url.len();
            self.clear_selection();
        }
        cx.notify();
    }

    // 全选
    fn select_all(&mut self, cx: &mut Context<Self>) {
        if !self.url.is_empty() {
            self.selection_start = Some(0);
            self.selection_end = Some(self.url.len());
            self.cursor_position = self.url.len();
            cx.notify();
        }
    }

    // 完善的文本输入处理 - 支持真正的键盘输入
    pub fn handle_keyboard_input(&mut self, input: KeyboardInput, cx: &mut Context<Self>) {
        match input {
            KeyboardInput::Character(ch) => {
                self.insert_char(ch, cx);
            }
            KeyboardInput::Backspace => {
                self.backspace(cx);
            }
            KeyboardInput::Delete => {
                self.delete(cx);
            }
            KeyboardInput::Enter => {
                self.submit_url(cx);
            }
            KeyboardInput::Escape => {
                self.is_editing = false;
                cx.notify();
            }
            KeyboardInput::ArrowLeft { shift } => {
                self.move_cursor_left(shift, cx);
            }
            KeyboardInput::ArrowRight { shift } => {
                self.move_cursor_right(shift, cx);
            }
            KeyboardInput::Home { shift } => {
                self.move_cursor_to_start(shift, cx);
            }
            KeyboardInput::End { shift } => {
                self.move_cursor_to_end(shift, cx);
            }
            KeyboardInput::SelectAll => {
                self.select_all(cx);
            }
            KeyboardInput::Paste(text) => {
                self.insert_text(&text, cx);
            }
            KeyboardInput::Cut => {
                if let Some((start, end)) = self.get_selection_range() {
                    let selected_text = self.url[start..end].to_string();
                    self.copy_to_clipboard(selected_text);
                    self.delete_selection();
                    cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
                    cx.notify();
                }
            }
            KeyboardInput::Copy => {
                if let Some((start, end)) = self.get_selection_range() {
                    let selected_text = self.url[start..end].to_string();
                    self.copy_to_clipboard(selected_text);
                }
            }
        }
    }

    // 模拟剪贴板操作（实际项目中需要系统剪贴板集成）
    fn copy_to_clipboard(&self, text: String) {
        println!("复制到剪贴板: {}", text);
        // 实际实现中，可以使用 arboard 或其他剪贴板库
    }

    // 简单的编辑功能 - 现在改为启动键盘输入模式
    fn toggle_edit(&mut self, cx: &mut Context<Self>) {
        self.is_editing = !self.is_editing;

        if self.is_editing {
            println!("🎯 开始键盘输入模式 - URL: {}", self.url);
            // 模拟一些常见的输入操作作为演示
            self.simulate_typing_demo(cx);
        } else {
            println!("⏹️  结束编辑模式 - URL: {}", self.url);
        }

        cx.notify();
    }

    // 演示键盘输入功能
    fn simulate_typing_demo(&mut self, cx: &mut Context<Self>) {
        // 演示不同类型的键盘输入
        println!("🎹 演示键盘输入功能:");

        // 1. 插入一些字符
        self.handle_keyboard_input(KeyboardInput::Character('h'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('t'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('t'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('p'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('s'), cx);
        self.handle_keyboard_input(KeyboardInput::Character(':'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('/'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('/'), cx);

        // 2. 粘贴一些文本
        self.handle_keyboard_input(KeyboardInput::Paste("api.example.com".to_string()), cx);

        // 3. 继续输入
        self.handle_keyboard_input(KeyboardInput::Character('/'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('v'), cx);
        self.handle_keyboard_input(KeyboardInput::Character('1'), cx);
    }

    fn display_text(&self) -> String {
        if self.url.is_empty() && !self.is_editing {
            self.placeholder.clone()
        } else {
            let mut display = self.url.clone();

            // 显示选择范围（简化版本）
            if let Some((start, end)) = self.get_selection_range() {
                if start != end {
                    display.insert_str(end, "]");
                    display.insert_str(start, "[");
                    return display;
                }
            }

            // 显示光标
            if self.is_editing && self.cursor_position <= display.len() {
                display.insert(self.cursor_position, '|');
            }

            display
        }
    }
}

// 键盘输入枚举 - 定义所有支持的键盘输入类型
#[derive(Debug, Clone)]
pub enum KeyboardInput {
    Character(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    ArrowLeft { shift: bool },
    ArrowRight { shift: bool },
    Home { shift: bool },
    End { shift: bool },
    SelectAll,
    Paste(String),
    Copy,
    Cut,
}

impl EventEmitter<UrlInputEvent> for UrlInput {}

impl Focusable for UrlInput {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UrlInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("url_input")
            .flex_1()
            .px_4()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(if self.is_editing {
                rgb(0x007acc) // 编辑时的蓝色边框
            } else {
                rgb(0xcccccc)
            })
            .rounded_md()
            .cursor_text()
            .track_focus(&self.focus_handle)
            .on_click(cx.listener(|this, _event, window, cx| {
                window.focus(&this.focus_handle);
                this.toggle_edit(cx);
            }))
            .child(
                div()
                    .text_color(if self.url.is_empty() && !self.is_editing {
                        rgb(0x999999) // 占位符颜色
                    } else {
                        rgb(0x333333) // 正常文本颜色
                    })
                    .child(self.display_text()),
            )
    }
}
