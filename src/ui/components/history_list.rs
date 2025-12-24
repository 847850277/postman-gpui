use crate::models::{HistoryEntry, Request};
use gpui::{
    div, px, rgb, Context, EventEmitter, InteractiveElement, IntoElement, ParentElement, Render,
    Rgba, StatefulInteractiveElement, Styled, Window,
};

/// Get color for HTTP method
fn get_method_color(method: &str) -> Rgba {
    match method.to_uppercase().as_str() {
        "GET" => rgb(0x0028_a745),
        "POST" => rgb(0x0000_7acc),
        "PUT" => rgb(0x00fd_7e14),
        "DELETE" => rgb(0x00dc_3545),
        "PATCH" => rgb(0x006f_42c1),
        _ => rgb(0x006c_757d),
    }
}

/// Color for additional info text (headers/body indicators)
const COLOR_INFO_TEXT: u32 = 0x0099_9999;

/// Event emitted when a history item is clicked
#[derive(Debug, Clone)]
pub enum HistoryListEvent {
    RequestSelected(Request),
}

/// History list component for displaying request history
pub struct HistoryList {
    entries: Vec<HistoryEntry>,
    selected_index: Option<usize>,
}

impl EventEmitter<HistoryListEvent> for HistoryList {}

impl HistoryList {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: None,
        }
    }

    /// Update the history entries
    pub fn set_entries(&mut self, entries: Vec<HistoryEntry>, cx: &mut Context<Self>) {
        self.entries = entries;
        cx.notify();
    }

    /// Get the currently selected request
    pub fn selected_request(&self) -> Option<&Request> {
        self.selected_index
            .and_then(|idx| self.entries.get(idx))
            .map(|entry| &entry.request)
    }

    /// Clear all entries
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.selected_index = None;
        cx.notify();
    }

    fn on_item_clicked(&mut self, index: usize, cx: &mut Context<Self>) -> HistoryListEvent {
        self.selected_index = Some(index);
        cx.notify();

        if let Some(entry) = self.entries.get(index) {
            println!("🔘 History item clicked:");
            println!("   Index: {}", index);
            println!("   Method: {}", entry.request.method);
            println!("   URL: {}", entry.request.url);
            println!("   Headers: {}", entry.request.headers.len());
            if let Some(ref body) = entry.request.body {
                println!("   Body: {} bytes", body.len());
            }
            println!("   ➡️ Loading request into form...");
            HistoryListEvent::RequestSelected(entry.request.clone())
        } else {
            // Log the error if index is out of bounds (shouldn't happen, but handle gracefully)
            eprintln!(
                "Warning: Attempted to select history item at invalid index {} (entries length: {})",
                index,
                self.entries.len()
            );
            HistoryListEvent::RequestSelected(Request::default())
        }
    }
}

impl Render for HistoryList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("history-list")
            .flex()
            .flex_col()
            .w_64() // Fixed width for sidebar
            .h_full()
            .bg(rgb(0x00f8_f9fa))
            .border_r_1()
            .border_color(rgb(0x00cc_cccc))
            .overflow_scroll()
            .child(
                // Header
                div()
                    .px_3()
                    .py_3()
                    .bg(rgb(0x00e9_ecef))
                    .border_b_1()
                    .border_color(rgb(0x00cc_cccc))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Request History"),
                    ),
            )
            .child(
                // History items
                div()
                    .flex()
                    .flex_col()
                    .children(if self.entries.is_empty() {
                        vec![div()
                            .px_3()
                            .py_4()
                            .text_size(px(12.0))
                            .text_color(rgb(0x006c_757d))
                            .child("No requests yet")]
                    } else {
                        self.entries
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| {
                                let is_selected = self.selected_index == Some(index);
                                let method_color = get_method_color(&entry.request.method);

                                let bg_color = if is_selected {
                                    rgb(0x00e7_f1ff)
                                } else {
                                    rgb(0x00f8_f9fa)
                                };

                                div()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(0x00de_e2e6))
                                    .cursor_pointer()
                                    .bg(bg_color)
                                    .hover(|style| {
                                        if is_selected {
                                            style.bg(rgb(0x00e7_f1ff))
                                        } else {
                                            style.bg(rgb(0x00ff_ffff))
                                        }
                                    })
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            let event = this.on_item_clicked(index, cx);
                                            cx.emit(event);
                                        }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .px_1()
                                                            .text_size(px(10.0))
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .text_color(method_color)
                                                            .child(entry.request.method.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(rgb(0x006c_757d))
                                                            .child(entry.formatted_time()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .overflow_hidden()
                                                    .child(entry.name.clone()),
                                            )
                                            .children({
                                                let has_headers = !entry.request.headers.is_empty();
                                                let has_body = entry.request.body.is_some();

                                                if has_headers || has_body {
                                                    Some(
                                                        div()
                                                            .text_size(px(9.0))
                                                            .text_color(rgb(COLOR_INFO_TEXT))
                                                            .child(format!(
                                                                "{}{}",
                                                                if has_headers {
                                                                    format!(
                                                                        "{} headers",
                                                                        entry.request.headers.len()
                                                                    )
                                                                } else {
                                                                    String::new()
                                                                },
                                                                if has_body {
                                                                    if has_headers {
                                                                        " • has body"
                                                                    } else {
                                                                        "has body"
                                                                    }
                                                                } else {
                                                                    ""
                                                                }
                                                            )),
                                                    )
                                                } else {
                                                    None
                                                }
                                            }),
                                    )
                            })
                            .collect()
                    }),
            )
    }
}
