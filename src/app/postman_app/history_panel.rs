use crate::app::WorkspaceViewModel;
use crate::models::{HistoryEntry, Request};
use crate::ui::components::input::header_input::{HeaderInput, HeaderInputEvent};
use crate::ui::theme::{
    method_color, ACCENT_SOFT, FONT_HEADING, FONT_UI, LINE, MUTED, PANEL, PANEL_ALT, SUBTEXT, TEXT,
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, EventEmitter,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window,
};

const HISTORY_SELECTED_BORDER: u32 = 0x00f2_b89f;

/// Event emitted when a history item is clicked
#[derive(Debug, Clone)]
pub enum HistoryListEvent {
    RequestSelected(HistoryEntry),
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
        let search_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Filter history")
                .with_embedded_chrome(true)
                .with_font_family(FONT_UI)
        });
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
                .searchable_text()
                .to_lowercase()
                .contains(query)
    }

    fn on_item_clicked(&mut self, index: usize, cx: &mut Context<Self>) -> HistoryListEvent {
        self.selected_index = Some(index);
        cx.notify();

        let entry = self.view_model.read(cx).history().get(index).cloned();

        if let Some(entry) = entry {
            tracing::debug!(
                index,
                method = %entry.request.method,
                url = %crate::utils::log::display_url_for_log(&entry.request.url),
                "history item selected"
            );
            HistoryListEvent::RequestSelected(entry)
        } else {
            tracing::warn!(
                index,
                entries = self.view_model.read(cx).history_len(),
                "history item index is out of range"
            );
            HistoryListEvent::RequestSelected(HistoryEntry::new(Request::default(), String::new()))
        }
    }

    fn request_name(entry: &HistoryEntry) -> String {
        let without_scheme = entry
            .request
            .url
            .split_once("://")
            .map(|(_, value)| value)
            .unwrap_or(&entry.request.url);
        let without_fragment = without_scheme.split('#').next().unwrap_or(without_scheme);
        let without_query = without_fragment
            .split('?')
            .next()
            .unwrap_or(without_fragment)
            .trim_end_matches('/');
        if without_query.is_empty() {
            entry.name.clone()
        } else {
            without_query.to_string()
        }
    }

    fn response_detail(entry: &HistoryEntry) -> String {
        match (entry.status, entry.elapsed_ms, entry.response_size) {
            (Some(status), Some(elapsed_ms), Some(response_size)) => format!(
                "{status} · {elapsed_ms} ms · {}",
                Self::format_response_size(response_size)
            ),
            _ => entry.formatted_time(),
        }
    }

    fn format_response_size(size: usize) -> String {
        if size < 1024 {
            format!("{size} B")
        } else {
            format!("{:.1} KB", size as f64 / 1024.0)
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
        let has_visible_entries = !visible_entries.is_empty();
        div()
            .id("history-list")
            .debug_selector(|| "history-panel".into())
            .flex()
            .flex_col()
            .w(px(320.0))
            .h_full()
            .flex_none()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(18.0))
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .debug_selector(|| "history-header".into())
                    .h(px(24.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .debug_selector(|| "history-title".into())
                            .font_family(FONT_HEADING)
                            .text_size(px(20.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("History"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "history-options".into())
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(FONT_UI)
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(MUTED))
                            .child("•••"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "history-subtitle".into())
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .text_color(rgb(SUBTEXT))
                    .child("Requests in this workspace"),
            )
            .child(
                div()
                    .debug_selector(|| "history-search-input".into())
                    .h(px(38.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(11.0))
                    .rounded(px(8.0))
                    .bg(rgb(PANEL_ALT))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .size(px(15.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(FONT_UI)
                            .text_size(px(15.0))
                            .text_color(rgb(MUTED))
                            .child("⌕"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .child(self.search_input.clone()),
                    ),
            )
            .when(has_visible_entries, |panel| {
                panel.child(
                    div()
                        .debug_selector(|| "history-date".into())
                        .h(px(12.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .font_family(FONT_UI)
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(MUTED))
                        .child("TODAY"),
                )
            })
            .child(
                div()
                    .id("history-scroll")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
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
                                let is_selected = self
                                    .selected_index
                                    .map_or(index == 0, |selected| selected == index);
                                let method_color = rgb(method_color(entry.request.method));
                                let request_name = Self::request_name(&entry);
                                let response_detail = Self::response_detail(&entry);

                                let bg_color = if is_selected {
                                    rgb(ACCENT_SOFT)
                                } else {
                                    rgb(PANEL)
                                };

                                div()
                                    .debug_selector(move || format!("history-item-{index}"))
                                    .h(px(58.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .px(px(10.0))
                                    .rounded(px(9.0))
                                    .border_1()
                                    .border_color(rgb(if is_selected {
                                        HISTORY_SELECTED_BORDER
                                    } else {
                                        PANEL
                                    }))
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
                                            .debug_selector(move || {
                                                format!("history-method-{index}")
                                            })
                                            .w(px(48.0))
                                            .h(px(24.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(6.0))
                                            .bg(rgb(if is_selected { PANEL } else { PANEL_ALT }))
                                            .font_family(FONT_UI)
                                            .text_size(px(10.0))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(method_color)
                                            .child(entry.request.method.to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .font_family(FONT_UI)
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        format!("history-request-name-{index}")
                                                    })
                                                    .overflow_hidden()
                                                    .text_size(px(12.0))
                                                    .font_weight(if is_selected {
                                                        gpui::FontWeight::BOLD
                                                    } else {
                                                        gpui::FontWeight::SEMIBOLD
                                                    })
                                                    .text_color(rgb(TEXT))
                                                    .child(request_name),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        format!("history-response-detail-{index}")
                                                    })
                                                    .overflow_hidden()
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(SUBTEXT))
                                                    .child(response_detail),
                                            ),
                                    )
                            })
                            .collect()
                    }),
            )
    }
}
