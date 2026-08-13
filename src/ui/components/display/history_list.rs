use crate::models::{HistoryEntry, HttpMethod, Request};
use crate::ui::theme::{
    ACCENT, ACCENT_DARK, ACCENT_SOFT, FONT_HEADING, FONT_UI, LINE, MUTED, PANEL, PANEL_ALT,
    SUBTEXT, TEXT,
};
use gpui::{
    div, px, rgb, Context, EventEmitter, InteractiveElement, IntoElement, ParentElement, Render,
    Rgba, StatefulInteractiveElement, Styled, Window,
};

/// Get color for HTTP method
fn get_method_color(method: HttpMethod) -> Rgba {
    match method {
        HttpMethod::GET => rgb(0x0016_a34a),
        HttpMethod::POST => rgb(ACCENT),
        HttpMethod::PUT => rgb(0x0025_63eb),
        HttpMethod::DELETE => rgb(0x00dc_2626),
        HttpMethod::PATCH => rgb(0x007c_3aed),
        HttpMethod::HEAD => rgb(SUBTEXT),
        HttpMethod::OPTIONS => rgb(SUBTEXT),
    }
}

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
            tracing::info!(
                "🔘 History item clicked: Index: {}, Method: {}, URL: {}",
                index,
                entry.request.method,
                entry.request.url
            );
            tracing::info!("   Headers: {}", entry.request.headers.len());
            if let Some(ref body) = entry.request.body {
                tracing::info!("   Body: {} bytes", body.len());
            }
            tracing::info!("   ➡️ Loading request into form...");
            HistoryListEvent::RequestSelected(entry.request.clone())
        } else {
            // Log the error if index is out of bounds (shouldn't happen, but handle gracefully)
            tracing::info!(
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
            .debug_selector(|| "history-panel".into())
            .flex()
            .flex_col()
            .w(px(320.0))
            .h_full()
            .flex_none()
            .gap_3()
            .p_4()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .font_family(FONT_HEADING)
                    .text_size(px(20.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(TEXT))
                    .child("History"),
            )
            .child(
                div()
                    .font_family(FONT_UI)
                    .text_size(px(13.0))
                    .text_color(rgb(MUTED))
                    .child("Recent requests and responses"),
            )
            .child(
                div()
                    .h(px(40.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .rounded_lg()
                    .bg(rgb(PANEL_ALT))
                    .border_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(13.0))
                    .text_color(rgb(MUTED))
                    .child("Search history"),
            )
            .child(
                div()
                    .id("history-scroll")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py_1()
                    .overflow_scroll()
                    .children(if self.entries.is_empty() {
                        vec![div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .px_3()
                            .py_4()
                            .rounded_lg()
                            .bg(rgb(PANEL_ALT))
                            .font_family(FONT_UI)
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child("No requests yet"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(MUTED))
                                    .child("Completed requests will appear here."),
                            )]
                    } else {
                        self.entries
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| {
                                let is_selected = self.selected_index == Some(index);
                                let method_color = get_method_color(entry.request.method);

                                let bg_color = if is_selected {
                                    rgb(ACCENT_SOFT)
                                } else {
                                    rgb(PANEL)
                                };

                                div()
                                    .h(px(48.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .px_3()
                                    .rounded_lg()
                                    .cursor_pointer()
                                    .bg(bg_color)
                                    .hover(|style| {
                                        if is_selected {
                                            style.bg(rgb(ACCENT_SOFT))
                                        } else {
                                            style.bg(rgb(PANEL_ALT))
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
                                            .w_full()
                                            .font_family(FONT_UI)
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .text_color(method_color)
                                                            .child(
                                                                entry.request.method.to_string(),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(rgb(MUTED))
                                                            .child(entry.formatted_time()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(if is_selected {
                                                        ACCENT_DARK
                                                    } else {
                                                        TEXT
                                                    }))
                                                    .overflow_hidden()
                                                    .child(entry.name.clone()),
                                            ),
                                    )
                            })
                            .collect()
                    }),
            )
    }
}
