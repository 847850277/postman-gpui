//! Request discovery surfaces share domain rows; Kit owns modal and list behavior.
use super::{PostmanApp, ToolDialog};
use crate::{
    app::{FocusGlobalSearch, HistoryStorageStatus, RequestTabId, WorkspaceViewModel},
    models::{HistoryEntry, HttpMethod},
    ui::theme::{
        method_color, metrics as m, ACCENT, ERROR, FONT_MONO, FONT_UI, LINE, MUTED, OK, SUBTEXT,
        TEXT,
    },
};
use chrono::{Local, NaiveDate};
use gpui::{
    actions, div, prelude::FluentBuilder, App, AppContext, Context, Entity, EventEmitter,
    Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::actions::{Confirm, SelectDown, SelectUp};
use gpui_kit::component::{
    button::{Button, ButtonVariant, ButtonVariants},
    dialog::DialogButtonProps,
    input::{Input, InputEvent, InputState},
    list::{List, ListDelegate, ListEvent, ListItem, ListState},
    Disableable, Icon, IndexPath, Sizable, WindowExt,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FinderKind {
    History,
    Search,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RequestTarget {
    Request(RequestTabId),
    History(String),
}
#[derive(Clone)]
pub(super) enum RequestFinderEvent {
    Activate(RequestTarget),
    Refresh,
    Clear,
    Closed,
}
actions!(request_finder, [NextResult, PreviousResult, OpenResult]);

struct RequestRow {
    target: RequestTarget,
    method: HttpMethod,
    name: String,
    url: String,
    detail: String,
    time: String,
    status: Option<u16>,
    selector: String,
}
struct RequestGroup {
    title: String,
    selector: &'static str,
    rows: Vec<RequestRow>,
}
struct RequestResults {
    compact: bool,
    groups: Vec<RequestGroup>,
    selected: Option<RequestTarget>,
    empty: &'static str,
    empty_selector: &'static str,
}
impl RequestResults {
    fn row(&self, ix: IndexPath) -> Option<&RequestRow> {
        self.groups.get(ix.section)?.rows.get(ix.row)
    }
    fn selected_index(&self) -> Option<IndexPath> {
        self.groups.iter().enumerate().find_map(|(section, g)| {
            g.rows
                .iter()
                .position(|r| Some(&r.target) == self.selected.as_ref())
                .map(|row| IndexPath::new(row).section(section))
        })
    }
}
impl ListDelegate for RequestResults {
    type Item = ListItem;
    fn sections_count(&self, _: &App) -> usize {
        self.groups.len().max(1)
    }
    fn items_count(&self, section: usize, _: &App) -> usize {
        self.groups.get(section).map_or(0, |g| g.rows.len())
    }
    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix.and_then(|ix| self.row(ix)).map(|r| r.target.clone());
    }
    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.row(ix)?;
        let id: SharedString = match &row.target {
            RequestTarget::Request(id) => format!("search-request-{id:?}").into(),
            RequestTarget::History(id) => format!("history-{id}").into(),
        };
        let selector = row.selector.clone();
        let suffix = row
            .selector
            .strip_prefix("history-item-")
            .unwrap_or(&row.selector);
        let method_selector = format!("history-method-{suffix}");
        let detail_selector = format!("history-response-detail-{suffix}");
        let status_selector = format!("history-status-{}-{suffix}", row.status.unwrap_or_default());
        Some(
            ListItem::new(id)
                .role(gpui::Role::Button)
                .debug_selector(move || selector.clone())
                .accessibility_label(format!("Open {} {}", row.method, row.name))
                .px_3()
                .py_3()
                .h(gpui::rems(if self.compact { 3.5 } else { 5.0 }))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_2()
                        .w_full()
                        .min_w_0()
                        .child(
                            div()
                                .debug_selector(move || method_selector.clone())
                                .w(gpui::rems(if self.compact { 38. / 16. } else { 34. / 16. }))
                                .flex_none()
                                .pt_0p5()
                                .font_family(FONT_MONO)
                                .text_size(m::CAPTION)
                                .text_color(method_color(row.method).resolve(cx))
                                .child(row.method.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(if self.compact {
                                            m::LABEL
                                        } else {
                                            m::COMPACT_LABEL
                                        })
                                        .font_weight(m::MEDIUM)
                                        .text_color(TEXT.resolve(cx))
                                        .child(row.name.clone()),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .font_family(FONT_MONO)
                                        .text_size(m::CAPTION)
                                        .text_color(MUTED.resolve(cx))
                                        .child(row.url.clone()),
                                )
                                .when(!self.compact, |d| {
                                    d.child(
                                        div()
                                            .truncate()
                                            .text_size(m::CAPTION)
                                            .text_color(
                                                match row.status {
                                                    Some(s) if s >= 400 => ERROR,
                                                    Some(_) => OK,
                                                    None => MUTED,
                                                }
                                                .resolve(cx),
                                            )
                                            .debug_selector(move || detail_selector.clone())
                                            .child(
                                                div()
                                                    .debug_selector(move || status_selector.clone())
                                                    .child(row.detail.clone()),
                                            ),
                                    )
                                }),
                        )
                        .when(!self.compact, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(m::CAPTION)
                                    .text_color(MUTED.resolve(cx))
                                    .child(row.time.clone()),
                            )
                        }),
                ),
        )
    }
    fn render_section_header(
        &mut self,
        section: usize,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        self.groups.get(section).map(|group| {
            let selector = group.selector;
            div()
                .debug_selector(move || selector.into())
                .h_8()
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .text_size(m::CAPTION)
                .text_color(MUTED.resolve(cx))
                .child(group.title.clone())
                .child(format!(
                    "{} {}",
                    group.rows.len(),
                    if group.rows.len() == 1 {
                        "request"
                    } else {
                        "requests"
                    }
                ))
        })
    }
    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let selector = self.empty_selector;
        div()
            .debug_selector(move || selector.into())
            .p_6()
            .text_size(m::LABEL)
            .text_color(MUTED.resolve(cx))
            .child(self.empty)
    }
}

pub(super) struct RequestFinder {
    kind: FinderKind,
    model: Entity<WorkspaceViewModel>,
    input: Entity<InputState>,
    results: Entity<ListState<RequestResults>>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<RequestFinderEvent> for RequestFinder {}
impl RequestFinder {
    pub(super) fn new(
        kind: FinderKind,
        model: Entity<WorkspaceViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys([
            KeyBinding::new(
                "space",
                Confirm { secondary: false },
                Some("RequestFinder > List && !Input"),
            ),
            KeyBinding::new("down", NextResult, Some("RequestFinder > Input")),
            KeyBinding::new("up", PreviousResult, Some("RequestFinder > Input")),
            KeyBinding::new("enter", OpenResult, Some("RequestFinder > Input")),
        ]);
        // Kit 0.7.1 searchable List resets selection after each search and keeps its
        // query InputState private. A retained Kit Input lets this feature preserve domain-ID
        // selection and its single focus border; Kit List still owns navigation and scrolling.
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(if kind == FinderKind::History {
                "Filter history…"
            } else {
                "Find a request…"
            })
        });
        let results = cx.new(|cx| {
            let list = ListState::new(
                RequestResults {
                    compact: kind == FinderKind::Search,
                    groups: vec![],
                    selected: None,
                    empty: "",
                    empty_selector: "finder-empty",
                },
                window,
                cx,
            );
            let _ = list.focus_handle(cx).tab_stop(true);
            list
        });
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_results(window, cx);
                }
            }),
            cx.observe_in(&model, window, |this, _, window, cx| {
                this.refresh_results(window, cx)
            }),
            cx.subscribe_in(&results, window, |this, _, event, _, cx| {
                if matches!(event, ListEvent::Confirm(_)) {
                    if let Some(target) = this.results.read(cx).delegate().selected.clone() {
                        cx.emit(RequestFinderEvent::Activate(target));
                    }
                }
            }),
        ];
        let mut this = Self {
            kind,
            model,
            input,
            results,
            _subscriptions: subscriptions,
        };
        this.refresh_results(window, cx);
        this
    }
    pub(super) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Start each discovery session at its first result (the newest history entry).
        // Once open, refresh_results preserves selection by domain ID.
        self.results
            .update(cx, |list, _| list.delegate_mut().selected = None);
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh_results(window, cx);
        let entity = cx.entity();
        let owner = cx.weak_entity();
        let history = self.kind == FinderKind::History;
        window.open_dialog(cx, move |dialog, window, _| {
            let entity = entity.clone();
            let owner = owner.clone();
            let height = (window.rem_size() * if history { 40. } else { 34. })
                .min(window.viewport_size().height - window.rem_size() * 2.);
            dialog
                .w(window.rem_size() * if history { 30. } else { 35. })
                .h(height)
                .margin_top((window.viewport_size().height - height) / 2.)
                .p_0()
                .gap_0()
                .close_button(false)
                .content(move |content, _, _| content.p_0().min_h_0().child(entity.clone()))
                .on_close(move |_, _, cx| {
                    let _ = owner.update(cx, |_, cx| cx.emit(RequestFinderEvent::Closed));
                })
                // Enter is owned by result activation, never the dialog's default OK.
                .on_ok(|_, _, _| false)
        });
        self.input.read(cx).focus_handle(cx).focus(window, cx);
    }
    fn refresh_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().trim().to_lowercase();
        let model = self.model.read(cx);
        let mut groups = Vec::<RequestGroup>::new();
        let empty;
        if self.kind == FinderKind::History {
            empty = if model.history().is_empty() {
                "No requests yet. Completed requests will appear here."
            } else {
                "No matching requests. Try another method, URL, header, or body value."
            };
            for (ix, entry) in model
                .history()
                .iter()
                .enumerate()
                .filter(|(_, e)| matches_history(e, &query))
            {
                let date = entry.timestamp.with_timezone(&Local).date_naive();
                let title = date_label(date);
                if groups.last().is_none_or(|g| g.title != title) {
                    groups.push(RequestGroup {
                        title,
                        selector: "history-date",
                        rows: vec![],
                    });
                }
                groups
                    .last_mut()
                    .unwrap()
                    .rows
                    .push(history_row(entry, format!("history-item-{ix}")));
            }
        } else {
            empty = if query.is_empty() {
                "Type to search open requests and history."
            } else {
                "No matching requests. Try another name, method, or URL."
            };
            let projection = model.global_search_results(&query);
            if !projection.requests().is_empty() {
                groups.push(RequestGroup {
                    title: "Open requests".into(),
                    selector: "global-search-requests-group",
                    rows: projection
                        .requests()
                        .iter()
                        .enumerate()
                        .map(|(ix, r)| RequestRow {
                            target: RequestTarget::Request(r.tab_id),
                            method: r.method,
                            name: r.display_name.clone(),
                            url: r.url.clone(),
                            detail: "Open request".into(),
                            time: String::new(),
                            status: None,
                            selector: format!("global-search-request-result-{ix}"),
                        })
                        .collect(),
                });
            }
            if !projection.history().is_empty() {
                groups.push(RequestGroup {
                    title: "History".into(),
                    selector: "global-search-history-group",
                    rows: projection
                        .history()
                        .iter()
                        .enumerate()
                        .filter_map(|(ix, r)| {
                            model
                                .history()
                                .iter()
                                .find(|e| e.id == r.entry_id)
                                .map(|e| {
                                    history_row(e, format!("global-search-history-result-{ix}"))
                                })
                        })
                        .collect(),
                });
            }
        }
        self.results.update(cx, |list, cx| {
            list.delegate_mut().groups = groups;
            list.delegate_mut().empty = empty;
            list.delegate_mut().empty_selector = if self.kind == FinderKind::History {
                "history-empty-state"
            } else {
                "global-search-empty"
            };
            let selected = list.delegate().selected_index().or_else(|| {
                list.delegate()
                    .groups
                    .iter()
                    .enumerate()
                    .find(|(_, g)| !g.rows.is_empty())
                    .map(|(s, _)| IndexPath::new(0).section(s))
            });
            list.set_selected_index(selected, window, cx);
            list.scroll_to_selected_item(window, cx);
            cx.notify();
        });
        cx.notify();
    }
    fn confirm_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, window, _| {
            let owner = owner.clone();
            let description = "Delete all stored requests and historical responses. \
                               This cannot be undone. Open request drafts stay unchanged.";
            dialog
                .title("Clear all history?")
                .w(window.rem_size() * 28.)
                .description(description)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Clear history")
                        .ok_variant(ButtonVariant::Danger),
                )
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |_, cx| cx.emit(RequestFinderEvent::Clear));
                    true
                })
        });
    }
    fn forward(&self, action: &dyn gpui::Action, window: &mut Window, cx: &mut Context<Self>) {
        self.results
            .read(cx)
            .focus_handle(cx)
            .dispatch_action(action, window, cx);
    }
}
impl Render for RequestFinder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = self.kind == FinderKind::History;
        let model = self.model.read(cx);
        let busy = matches!(
            model.history_storage_status(),
            HistoryStorageStatus::Loading { .. }
        );
        let count = model.history_len();
        let (storage_selector, storage_text, color) = match model.history_storage_status() {
            HistoryStorageStatus::Loading { stage } => (
                "history-storage-loading",
                format!("Loading {stage}…"),
                MUTED,
            ),
            HistoryStorageStatus::Ready { skipped_rows: 0 } => {
                ("history-storage-ready", "Stored in SQLite".into(), MUTED)
            }
            HistoryStorageStatus::Ready { skipped_rows } => (
                "history-storage-ready-with-warnings",
                format!("Stored in SQLite · {skipped_rows} invalid rows skipped"),
                MUTED,
            ),
            HistoryStorageStatus::Error { stage, message } => (
                "history-storage-error",
                format!("History unavailable during {stage}: {message}"),
                ERROR,
            ),
        };
        let selector = if history {
            "history-panel"
        } else {
            "global-search-dialog"
        };
        let input_selector = if history {
            "history-search-input"
        } else {
            "global-search-input"
        };
        let close_selector = if history {
            "history-close"
        } else {
            "global-search-close"
        };
        let close = Button::new(close_selector)
            .debug_selector(move || close_selector.into())
            .ghost()
            .small()
            .icon(IconName::Close)
            .accessibility_label("Close dialog")
            .on_click(cx.listener(|_, _, w, cx| {
                w.close_dialog(cx);
                cx.emit(RequestFinderEvent::Closed);
            }));
        // A single focused border matches the prototype without nesting two input frames.
        let input = Input::new(&self.input)
            .prefix(Icon::new(IconName::Search).size_4())
            .cleanable(true)
            .focus_bordered(false)
            .border_color(
                if self.input.read(cx).focus_handle(cx).is_focused(window) {
                    ACCENT
                } else {
                    LINE
                }
                .resolve(cx),
            );
        let input = div()
            .debug_selector(move || input_selector.into())
            .flex_1()
            .min_w_0()
            .child(input);
        let mut root = div()
            .id(selector)
            .debug_selector(move || selector.into())
            .key_context("RequestFinder")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .font_family(FONT_UI)
            .text_size(m::BODY)
            .on_action(cx.listener(|this, _: &NextResult, w, cx| this.forward(&SelectDown, w, cx)))
            .on_action(
                cx.listener(|this, _: &PreviousResult, w, cx| this.forward(&SelectUp, w, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenResult, w, cx| {
                this.forward(&Confirm { secondary: false }, w, cx)
            }));
        if history {
            root = root
                .child(
                    div()
                        .debug_selector(|| "finder-header".into())
                        .h_16()
                        .px_5()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .border_b_1()
                        .border_color(LINE.resolve(cx))
                        .child(Icon::new(IconName::RotateCcw).size_4())
                        .child(div().font_weight(m::BOLD).child("History"))
                        .child(
                            div()
                                .px_1()
                                .border_1()
                                .border_color(LINE.resolve(cx))
                                .rounded_sm()
                                .text_size(m::CAPTION)
                                .text_color(MUTED.resolve(cx))
                                .child(count.to_string()),
                        )
                        .child(div().flex_1())
                        .child(close),
                )
                .child(div().px_5().py_3().flex_none().child(input));
        } else {
            root = root.child(
                div()
                    .debug_selector(|| "finder-header".into())
                    .h_16()
                    .px_5()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(LINE.resolve(cx))
                    .child(input)
                    .child(close),
            );
        }
        root = root.child(
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(List::new(&self.results).px_3()),
        );
        if history {
            root = root.child(
                div()
                    .debug_selector(|| "history-footer".into())
                    .flex_none()
                    .px_5()
                    .py_3()
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .debug_selector(move || storage_selector.into())
                            .flex_1()
                            .min_w_0()
                            .text_size(m::CAPTION)
                            .text_color(color.resolve(cx))
                            .child(storage_text),
                    )
                    .child(
                        Button::new("history-refresh-button")
                            .debug_selector(|| "history-refresh-button".into())
                            .small()
                            .ghost()
                            .label("Refresh")
                            .disabled(busy)
                            .on_click(
                                cx.listener(|_, _, _, cx| cx.emit(RequestFinderEvent::Refresh)),
                            ),
                    )
                    .child(
                        Button::new("history-clear-button")
                            .debug_selector(|| "history-clear-button".into())
                            .small()
                            .ghost()
                            .label("Clear…")
                            .disabled(busy || count == 0)
                            .on_click(cx.listener(|this, _, w, cx| this.confirm_clear(w, cx))),
                    ),
            );
        } else {
            root = root.child(
                div()
                    .px_5()
                    .py_3()
                    .flex_none()
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
                    .text_size(m::CAPTION)
                    .text_color(SUBTEXT.resolve(cx))
                    .child("↑ ↓ Select    Enter Open    Esc Close"),
            );
        }
        root
    }
}
fn matches_history(entry: &HistoryEntry, query: &str) -> bool {
    query.is_empty()
        || entry.name.to_lowercase().contains(query)
        || entry.request.url.to_lowercase().contains(query)
        || entry
            .request
            .method
            .to_string()
            .to_lowercase()
            .contains(query)
        || entry
            .request
            .headers
            .iter()
            .any(|(k, v)| k.to_lowercase().contains(query) || v.to_lowercase().contains(query))
        || entry
            .request
            .body
            .searchable_text()
            .to_lowercase()
            .contains(query)
}
fn date_label(date: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if date == today {
        "Today".into()
    } else if Some(date) == today.pred_opt() {
        "Yesterday".into()
    } else {
        date.format("%b %d, %Y").to_string()
    }
}
fn history_row(entry: &HistoryEntry, selector: String) -> RequestRow {
    RequestRow {
        target: RequestTarget::History(entry.id.clone()),
        method: entry.request.method,
        name: entry.name.clone(),
        url: entry.request.url.clone(),
        detail: format!(
            "{} · {} · {}",
            entry
                .status
                .map_or_else(|| "Status unavailable".into(), |s| s.to_string()),
            entry
                .elapsed_ms
                .map_or_else(|| "Duration unavailable".into(), |n| format!("{n} ms")),
            entry
                .response_size
                .map_or_else(|| "Size unavailable".into(), |n| format!("{n} B"))
        ),
        time: entry
            .timestamp
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string(),
        status: entry.status,
        selector,
    }
}
impl PostmanApp {
    pub(super) fn focus_global_search(
        &mut self,
        _: &FocusGlobalSearch,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_global_search_focus(w, cx);
    }
    pub(super) fn begin_global_search_focus(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.open_finder(FinderKind::Search, w, cx);
    }
    pub(super) fn render_global_search(
        &self,
        _: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new("global-search-trigger")
            .debug_selector(|| "global-search-trigger".into())
            .small()
            .ghost()
            .w_64()
            .icon(IconName::Search)
            .label("Search requests and history…")
            .toggled(self.active_tool == Some(ToolDialog::Search))
            .on_click(cx.listener(|this, _, w, cx| this.begin_global_search_focus(w, cx)))
    }
}
