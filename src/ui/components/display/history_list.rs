use crate::app::WorkspaceViewModel;
use crate::models::{HistoryEntry, HttpMethod, Request};
use crate::ui::components::header_input::{HeaderInput, HeaderInputEvent};
use crate::ui::theme::{
    ACCENT, ACCENT_DARK, ACCENT_SOFT, FONT_HEADING, FONT_UI, LINE, MUTED, PANEL, PANEL_ALT,
    SUBTEXT, TEXT,
};
use gpui::{
    div, px, rgb, App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, Rgba, StatefulInteractiveElement, Styled, Subscription, Window,
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
    view_model: Entity<WorkspaceViewModel>,
    selected_index: Option<usize>,
    search_query: String,
    search_input: Entity<HeaderInput>,
    _search_subscription: Subscription,
    _view_model_subscription: Subscription,
}

impl EventEmitter<HistoryListEvent> for HistoryList {}

impl HistoryList {
    pub fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| HeaderInput::new(cx).with_placeholder("Search history"));
        let search_subscription = cx.subscribe(&search_input, Self::on_search_input_event);
        let view_model_subscription = cx.observe(&view_model, |_, _, cx| cx.notify());
        Self {
            view_model,
            selected_index: None,
            search_query: String::new(),
            search_input,
            _search_subscription: search_subscription,
            _view_model_subscription: view_model_subscription,
        }
    }

    fn on_search_input_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(query) = event {
            self.search_query = query.trim().to_lowercase();
            cx.notify();
        }
    }

    pub fn visible_entry_count(&self, cx: &App) -> usize {
        self.view_model
            .read(cx)
            .history()
            .iter()
            .filter(|entry| self.matches_query(entry))
            .count()
    }

    fn matches_query(&self, entry: &HistoryEntry) -> bool {
        if self.search_query.is_empty() {
            return true;
        }
        let query = &self.search_query;
        entry.name.to_lowercase().contains(query)
            || entry.request.url.to_lowercase().contains(query)
            || entry
                .request
                .method
                .to_string()
                .to_lowercase()
                .contains(query)
            || entry.request.headers.iter().any(|(key, value)| {
                key.to_lowercase().contains(query) || value.to_lowercase().contains(query)
            })
            || entry
                .request
                .body
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
    }

    fn on_item_clicked(&mut self, index: usize, cx: &mut Context<Self>) -> HistoryListEvent {
        self.selected_index = Some(index);
        cx.notify();

        let request = self
            .view_model
            .read(cx)
            .history()
            .get(index)
            .map(|entry| entry.request.clone());

        if let Some(request) = request {
            tracing::info!(
                "🔘 History item clicked: Index: {}, Method: {}, URL: {}",
                index,
                request.method,
                request.url
            );
            tracing::info!("   Headers: {}", request.headers.len());
            if let Some(ref body) = request.body {
                tracing::info!("   Body: {} bytes", body.len());
            }
            tracing::info!("   ➡️ Loading request into form...");
            HistoryListEvent::RequestSelected(request)
        } else {
            // Log the error if index is out of bounds (shouldn't happen, but handle gracefully)
            tracing::info!(
                "Warning: Attempted to select history item at invalid index {} (entries length: {})",
                index,
                self.view_model.read(cx).history_len()
            );
            HistoryListEvent::RequestSelected(Request::default())
        }
    }
}

impl Render for HistoryList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.view_model.read(cx).history().to_vec();
        let visible_entries: Vec<(usize, HistoryEntry)> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.matches_query(entry))
            .map(|(index, entry)| (index, entry.clone()))
            .collect();
        let has_history = !entries.is_empty();
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
                    .debug_selector(|| "history-search-input".into())
                    .h(px(40.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .rounded_lg()
                    .bg(rgb(PANEL_ALT))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(self.search_input.clone()),
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
                    .children(if visible_entries.is_empty() {
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
                                    .child(if has_history {
                                        "No matching requests"
                                    } else {
                                        "No requests yet"
                                    }),
                            )
                            .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(
                                if has_history {
                                    "Try another method, URL, header, or body value."
                                } else {
                                    "Completed requests will appear here."
                                },
                            ))]
                    } else {
                        visible_entries
                            .into_iter()
                            .map(|(index, entry)| {
                                let is_selected = self.selected_index == Some(index);
                                let method_color = get_method_color(entry.request.method);

                                let bg_color = if is_selected {
                                    rgb(ACCENT_SOFT)
                                } else {
                                    rgb(PANEL)
                                };

                                div()
                                    .debug_selector(move || format!("history-item-{index}").into())
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
