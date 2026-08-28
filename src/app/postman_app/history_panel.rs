use crate::app::{ActivateControl, HistoryStorageStatus, WorkspaceViewModel};
use crate::models::HistoryEntry;
use crate::ui::components::input::header_input::{HeaderInput, HeaderInputEvent};
use crate::ui::theme::{
    method_color, ACCENT_SOFT, ERROR, FONT_HEADING, FONT_UI, LINE, MUTED, OK, PANEL, PANEL_ALT,
    SUBTEXT, TEXT,
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    ParentElement, Render, Role, StatefulInteractiveElement, Styled, Subscription, Window,
};
use std::collections::{HashMap, HashSet};

const HISTORY_SELECTED_BORDER: u32 = 0x00f2_b89f;

actions!(
    history_list,
    [
        ActivateHistoryItem,
        FocusFirstHistoryItem,
        FocusNextHistoryItem,
        FocusPreviousHistoryItem
    ]
);

fn setup_history_list_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivateHistoryItem, Some("HistoryItem")),
        KeyBinding::new("space", ActivateHistoryItem, Some("HistoryItem")),
        KeyBinding::new("tab", FocusFirstHistoryItem, Some("HistorySearch")),
        KeyBinding::new("tab", FocusNextHistoryItem, Some("HistoryItem")),
        KeyBinding::new("down", FocusNextHistoryItem, Some("HistoryItem")),
        KeyBinding::new("shift-tab", FocusPreviousHistoryItem, Some("HistoryItem")),
        KeyBinding::new("up", FocusPreviousHistoryItem, Some("HistoryItem")),
    ]
}

/// Event emitted when a History command is activated.
#[derive(Debug, Clone)]
pub enum HistoryListEvent {
    RequestSelected(Box<HistoryEntry>),
    RefreshRequested,
    ClearRequested,
}

/// History list component for displaying request history
pub struct HistoryList {
    view_model: Entity<WorkspaceViewModel>,
    selected_entry_id: Option<String>,
    item_focus_handles: HashMap<String, FocusHandle>,
    search_query: String,
    search_input: Entity<HeaderInput>,
    refresh_focus_handle: FocusHandle,
    clear_focus_handle: FocusHandle,
    _search_subscription: Subscription,
    _view_model_subscription: Subscription,
}

impl EventEmitter<HistoryListEvent> for HistoryList {}

impl HistoryList {
    pub fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        cx.bind_keys(setup_history_list_key_bindings());
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
            selected_entry_id: None,
            item_focus_handles: HashMap::new(),
            search_query: String::new(),
            search_input,
            refresh_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            clear_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
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

    pub(super) fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    }

    /// Mouse, Enter, and Space all resolve the currently rendered row through this command.
    /// Stable IDs avoid replaying a different request if an async SQLite refresh reorders rows.
    fn activate_item(
        &mut self,
        entry_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<HistoryListEvent> {
        let entry = self
            .view_model
            .read(cx)
            .history()
            .iter()
            .find(|entry| entry.id == entry_id)
            .cloned();

        if let Some(entry) = entry {
            self.selected_entry_id = Some(entry.id.clone());
            cx.notify();
            tracing::debug!(
                entry_id,
                method = %entry.request.method,
                url = %crate::utils::log::display_url_for_log(&entry.request.url),
                "history item selected"
            );
            Some(HistoryListEvent::RequestSelected(Box::new(entry)))
        } else {
            tracing::warn!(
                entry_id,
                entries = self.view_model.read(cx).history_len(),
                "history item is no longer present"
            );
            None
        }
    }

    fn focus_first_item(
        &mut self,
        _: &FocusFirstHistoryItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let first_visible_id = self
            .view_model
            .read(cx)
            .history()
            .iter()
            .find(|entry| self.matches_query(entry))
            .map(|entry| entry.id.clone());
        if let Some(focus_handle) = first_visible_id
            .as_ref()
            .and_then(|entry_id| self.item_focus_handles.get(entry_id))
        {
            focus_handle.focus(window, cx);
        } else {
            window.focus_next(cx);
        }
    }

    fn focus_next_item(
        &mut self,
        _: &FocusNextHistoryItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
    }

    fn focus_previous_item(
        &mut self,
        _: &FocusPreviousHistoryItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.view_model.read(cx).history().to_vec();
        let retained_entry_ids = entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        self.item_focus_handles
            .retain(|entry_id, _| retained_entry_ids.contains(entry_id.as_str()));
        if self
            .selected_entry_id
            .as_ref()
            .is_some_and(|selected| !entries.iter().any(|entry| entry.id == *selected))
        {
            self.selected_entry_id = None;
        }
        let visible_entries: Vec<(usize, HistoryEntry)> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.matches_query(entry))
            .map(|(index, entry)| (index, entry.clone()))
            .collect();
        let has_history = !entries.is_empty();
        let has_visible_entries = !visible_entries.is_empty();
        let search_focused = self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let (storage_selector, storage_text, storage_color) =
            match self.view_model.read(cx).history_storage_status() {
                HistoryStorageStatus::Loading { stage } => (
                    "history-storage-loading",
                    format!("SQLite · Loading {stage}"),
                    MUTED,
                ),
                HistoryStorageStatus::Ready { skipped_rows: 0 } => {
                    ("history-storage-ready", "Stored in SQLite".to_string(), OK)
                }
                HistoryStorageStatus::Ready { skipped_rows } => (
                    "history-storage-ready-with-warnings",
                    format!("Stored in SQLite · {skipped_rows} invalid rows skipped"),
                    MUTED,
                ),
                HistoryStorageStatus::Error { stage, message } => (
                    "history-storage-error",
                    format!("SQLite unavailable during {stage}: {message}"),
                    ERROR,
                ),
            };
        div()
            .id("history-list")
            .debug_selector(|| "history-panel".into())
            .flex()
            .flex_col()
            .w_full()
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
                            .debug_selector(|| "history-actions".into())
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .id("history-refresh-button")
                                    .debug_selector(|| "history-refresh-button".into())
                                    .track_focus(&self.refresh_focus_handle)
                                    .key_context("KeyboardButton")
                                    .role(Role::Button)
                                    .aria_label("Refresh History")
                                    .h(px(24.0))
                                    .px(px(7.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .cursor_pointer()
                                    .font_family(FONT_UI)
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .hover(|style| style.bg(rgb(PANEL_ALT)).text_color(rgb(TEXT)))
                                    .when(self.refresh_focus_handle.is_focused(window), |button| {
                                        button.border_color(rgb(HISTORY_SELECTED_BORDER))
                                    })
                                    .on_action(cx.listener(
                                        |_this, _: &ActivateControl, _window, cx| {
                                            cx.emit(HistoryListEvent::RefreshRequested);
                                        },
                                    ))
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.refresh_focus_handle.focus(window, cx);
                                            cx.emit(HistoryListEvent::RefreshRequested);
                                        }),
                                    )
                                    .child("Refresh"),
                            )
                            .child(
                                div()
                                    .id("history-clear-button")
                                    .debug_selector(|| "history-clear-button".into())
                                    .track_focus(&self.clear_focus_handle)
                                    .key_context("KeyboardButton")
                                    .role(Role::Button)
                                    .aria_label("Clear History")
                                    .h(px(24.0))
                                    .px(px(7.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .cursor_pointer()
                                    .font_family(FONT_UI)
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .hover(|style| style.bg(rgb(PANEL_ALT)).text_color(rgb(ERROR)))
                                    .when(self.clear_focus_handle.is_focused(window), |button| {
                                        button.border_color(rgb(HISTORY_SELECTED_BORDER))
                                    })
                                    .on_action(cx.listener(
                                        |_this, _: &ActivateControl, _window, cx| {
                                            cx.emit(HistoryListEvent::ClearRequested);
                                        },
                                    ))
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.clear_focus_handle.focus(window, cx);
                                            cx.emit(HistoryListEvent::ClearRequested);
                                        }),
                                    )
                                    .child("Clear"),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(move || storage_selector.into())
                    .overflow_hidden()
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .text_color(rgb(storage_color))
                    .child(storage_text),
            )
            .child(
                div()
                    .id("history-search-shell")
                    .debug_selector(|| "history-search-input".into())
                    .key_context("HistorySearch")
                    .h(px(38.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(11.0))
                    .rounded(px(8.0))
                    .bg(rgb(PANEL_ALT))
                    .border_1()
                    .border_color(rgb(if search_focused {
                        HISTORY_SELECTED_BORDER
                    } else {
                        LINE
                    }))
                    .on_action(cx.listener(Self::focus_first_item))
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
                            .id("history-empty-state")
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
                                let focus_handle = self
                                    .item_focus_handles
                                    .entry(entry.id.clone())
                                    .or_insert_with(|| {
                                        cx.focus_handle().tab_index(0).tab_stop(true)
                                    })
                                    .clone();
                                let mouse_focus_handle = focus_handle.clone();
                                let focused = focus_handle.is_focused(window);
                                let is_selected = self
                                    .selected_entry_id
                                    .as_deref()
                                    .is_some_and(|selected| selected == entry.id);
                                let method_color = rgb(method_color(entry.request.method));
                                let request_name = Self::request_name(&entry);
                                let accessible_label = format!(
                                    "Replay {} {}",
                                    entry.request.method, request_name
                                );
                                let response_detail = Self::response_detail(&entry);
                                let response_status = entry.status;
                                let mouse_entry_id = entry.id.clone();
                                let keyboard_entry_id = entry.id.clone();

                                let bg_color = if is_selected {
                                    rgb(ACCENT_SOFT)
                                } else {
                                    rgb(PANEL)
                                };

                                div()
                                    .id(entry.id.clone())
                                    .debug_selector(move || format!("history-item-{index}"))
                                    .track_focus(&focus_handle)
                                    .key_context("HistoryItem")
                                    .role(Role::Button)
                                    .aria_label(accessible_label)
                                    .h(px(58.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .px(px(10.0))
                                    .rounded(px(9.0))
                                    .border_1()
                                    .border_color(rgb(if is_selected || focused {
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
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, window, cx| {
                                            mouse_focus_handle.focus(window, cx);
                                            if let Some(event) =
                                                this.activate_item(&mouse_entry_id, cx)
                                            {
                                                cx.emit(event);
                                            }
                                        }),
                                    )
                                    .on_action(cx.listener(
                                        move |this,
                                              _: &ActivateHistoryItem,
                                              _window,
                                              cx| {
                                            if let Some(event) =
                                                this.activate_item(&keyboard_entry_id, cx)
                                            {
                                                cx.emit(event);
                                            }
                                        },
                                    ))
                                    .on_action(cx.listener(Self::focus_next_item))
                                    .on_action(cx.listener(Self::focus_previous_item))
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
                                                    .child(
                                                        div()
                                                            .when_some(
                                                                response_status,
                                                                move |detail, status| {
                                                                    detail.debug_selector(move || {
                                                                        format!(
                                                                            "history-status-{status}-{index}"
                                                                        )
                                                                    })
                                                                },
                                                            )
                                                            .child(response_detail),
                                                    ),
                                            ),
                                    )
                            })
                            .collect()
                    }),
            )
    }
}
