use super::PostmanApp;
use crate::{
    app::{
        ActivateControl, ActivateGlobalSearchResult, DismissGlobalSearch, FocusGlobalSearch,
        GlobalSearchHistoryResult, GlobalSearchRequestResult, GlobalSearchResults, RequestTabId,
        SelectNextGlobalSearchResult, SelectPreviousGlobalSearchResult,
    },
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{
            method_color, ACCENT, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED,
            PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    anchored, canvas, deferred, div, point, prelude::FluentBuilder, px, rgb, Anchor, Context,
    Focusable, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement, Role,
    StatefulInteractiveElement, Styled, Window,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum GlobalSearchTarget {
    Request(RequestTabId),
    History(String),
}

impl PostmanApp {
    pub(super) fn on_global_search_input_event(
        &mut self,
        _input: gpui::Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(query) = event {
            self.global_search_query = query.clone();
            self.global_search_selected_index = 0;
            cx.notify();
        }
    }

    pub(super) fn focus_global_search(
        &mut self,
        _: &FocusGlobalSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_global_search_focus(window, cx);
    }

    fn begin_global_search_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_global_search_return_focus(window, cx);
        let search_focus = self.global_search_input.read(cx).focus_handle(cx);
        search_focus.focus(window, cx);
        cx.notify();
    }

    fn remember_global_search_return_focus(&mut self, window: &Window, cx: &Context<Self>) {
        let search_focus = self.global_search_input.read(cx).focus_handle(cx);
        let focused = window.focused(cx);
        let focus_is_inside_search = focused.as_ref().is_some_and(|focused| {
            focused == &search_focus || focused == &self.global_search_clear_focus
        });
        if !focus_is_inside_search {
            self.global_search_return_focus = focused.map(|focused| focused.downgrade());
        }
    }

    pub(super) fn select_next_global_search_result(
        &mut self,
        _: &SelectNextGlobalSearchResult,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result_count = self.current_global_search_results(cx).len();
        if result_count > 0 {
            let selected = self
                .global_search_selected_index
                .min(result_count.saturating_sub(1));
            self.global_search_selected_index = (selected + 1) % result_count;
            cx.notify();
        }
    }

    pub(super) fn select_previous_global_search_result(
        &mut self,
        _: &SelectPreviousGlobalSearchResult,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result_count = self.current_global_search_results(cx).len();
        if result_count > 0 {
            let selected = self
                .global_search_selected_index
                .min(result_count.saturating_sub(1));
            self.global_search_selected_index = if selected == 0 {
                result_count - 1
            } else {
                selected - 1
            };
            cx.notify();
        }
    }

    pub(super) fn activate_global_search_result(
        &mut self,
        _: &ActivateGlobalSearchResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let results = self.current_global_search_results(cx);
        let Some(target) = Self::target_at_index(
            &results,
            self.global_search_selected_index
                .min(results.len().saturating_sub(1)),
        ) else {
            return;
        };
        self.execute_global_search_target(target, window, cx);
    }

    pub(super) fn dismiss_global_search(
        &mut self,
        _: &DismissGlobalSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_global_search(cx);
        self.global_search_return_focus
            .take()
            .and_then(|focus| focus.upgrade())
            .unwrap_or_else(|| self.app_focus_handle.clone())
            .focus(window, cx);
        cx.notify();
    }

    fn clear_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_global_search(cx);
        self.global_search_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }

    fn reset_global_search(&mut self, cx: &mut Context<Self>) {
        self.global_search_query.clear();
        self.global_search_selected_index = 0;
        self.global_search_input
            .update(cx, |input, cx| input.project_content("", cx));
    }

    fn current_global_search_results(&self, cx: &Context<Self>) -> GlobalSearchResults {
        self.view_model
            .read(cx)
            .global_search_results(&self.global_search_query)
    }

    fn target_at_index(results: &GlobalSearchResults, index: usize) -> Option<GlobalSearchTarget> {
        if let Some(result) = results.requests().get(index) {
            return Some(GlobalSearchTarget::Request(result.tab_id));
        }
        results
            .history()
            .get(index.checked_sub(results.requests().len())?)
            .map(|result| GlobalSearchTarget::History(result.entry_id.clone()))
    }

    /// Mouse and keyboard activation converge here so both paths execute the same stable-ID
    /// command against the current ViewModel state.
    fn execute_global_search_target(
        &mut self,
        target: GlobalSearchTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_global_search(cx);
        self.global_search_return_focus = None;
        match target {
            GlobalSearchTarget::Request(tab_id) => {
                self.request_workspace.update(cx, |workspace, cx| {
                    workspace.activate_request_tab(tab_id, cx);
                    workspace.focus_active_request_tab(window, cx);
                });
            }
            GlobalSearchTarget::History(entry_id) => {
                let entry = self
                    .view_model
                    .read(cx)
                    .history()
                    .iter()
                    .find(|entry| entry.id == entry_id)
                    .cloned();
                if let Some(entry) = entry {
                    self.request_workspace.update(cx, |workspace, cx| {
                        workspace.load_history_entry(&entry, cx);
                        workspace.focus_url(window, cx);
                    });
                }
            }
        }
        cx.notify();
    }

    pub(super) fn render_global_search(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let search_focus = self.global_search_input.read(cx).focus_handle(cx);
        let focused = search_focus.is_focused(window);
        let has_query = !self.global_search_query.trim().is_empty();
        let app = cx.entity().clone();

        div()
            .id("global-search-shell")
            .debug_selector(|| "global-search-input".into())
            .relative()
            .key_context("GlobalSearch")
            .role(Role::ComboBox)
            .aria_label("Search requests and history")
            .aria_expanded(has_query)
            .w(px(430.0))
            .h(px(40.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if focused { INFO } else { LINE }))
            .bg(rgb(if focused { PANEL } else { PANEL_ALT }))
            .capture_any_mouse_down(cx.listener(
                |this, event: &gpui::MouseDownEvent, window, cx| {
                    if event.button == MouseButton::Left {
                        this.remember_global_search_return_focus(window, cx);
                    }
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.begin_global_search_focus(window, cx)),
            )
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(FONT_UI)
                    .text_size(px(16.0))
                    .text_color(rgb(if focused { INFO } else { MUTED }))
                    .child("⌕"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.global_search_input.clone()),
            )
            .when(has_query, |search| {
                search.child(
                    div()
                        .id("global-search-clear")
                        .debug_selector(|| "global-search-clear".into())
                        .track_focus(&self.global_search_clear_focus)
                        .key_context("KeyboardButton")
                        .role(Role::Button)
                        .aria_label("Clear global search")
                        .size(px(24.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .cursor_pointer()
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(MUTED))
                        .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT)))
                        .when(
                            self.global_search_clear_focus.is_focused(window),
                            |button| button.border_1().border_color(rgb(ACCENT)),
                        )
                        .child("×")
                        .on_action(cx.listener(|this, _: &ActivateControl, window, cx| {
                            cx.stop_propagation();
                            this.clear_global_search(window, cx);
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.clear_global_search(window, cx);
                            }),
                        ),
                )
            })
            .when(!has_query, |search| {
                search.child(
                    div()
                        .h(px(22.0))
                        .px_2()
                        .flex_none()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PANEL))
                        .font_family(FONT_UI)
                        .text_size(px(9.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(MUTED))
                        .child("⌘ K"),
                )
            })
            .child(
                canvas(
                    move |bounds, _, cx| app.update(cx, |app, _| app.global_search_bounds = bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(has_query, |search| {
                search.child(self.render_global_search_popover(cx))
            })
    }

    fn render_global_search_popover(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let results = self.current_global_search_results(cx);
        let selected_index = self
            .global_search_selected_index
            .min(results.len().saturating_sub(1));
        let bounds = self.global_search_bounds;

        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(bounds.left(), bounds.bottom() + px(8.0)))
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id("global-search-popover")
                        .debug_selector(|| "global-search-popover".into())
                        .occlude()
                        .w(px(430.0))
                        .max_h(px(440.0))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PANEL))
                        .shadow_lg()
                        .when(results.is_empty(), |popover| {
                            popover.child(self.render_global_search_empty(cx))
                        })
                        .when(!results.is_empty(), |popover| {
                            popover
                                .child(
                                    div()
                                        .id("global-search-results-scroll")
                                        .max_h(px(390.0))
                                        .overflow_y_scroll()
                                        .when(!results.requests().is_empty(), |list| {
                                            list.child(self.render_global_search_request_group(
                                                results.requests(),
                                                selected_index,
                                                cx,
                                            ))
                                        })
                                        .when(!results.history().is_empty(), |list| {
                                            list.child(self.render_global_search_history_group(
                                                results.history(),
                                                results.requests().len(),
                                                selected_index,
                                                cx,
                                            ))
                                        }),
                                )
                                .child(self.render_global_search_footer())
                        }),
                ),
        )
        .with_priority(1000)
    }

    fn render_global_search_request_group(
        &self,
        results: &[GlobalSearchRequestResult],
        selected_index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .debug_selector(|| "global-search-requests-group".into())
            .flex()
            .flex_col()
            .child(Self::render_global_search_group_heading(
                "OPEN REQUESTS",
                results.len(),
            ))
            .children(results.iter().cloned().enumerate().map(|(index, result)| {
                let target = GlobalSearchTarget::Request(result.tab_id);
                self.render_global_search_result_row(
                    "global-search-request-result",
                    index,
                    result.method,
                    result.display_name,
                    if result.url.is_empty() {
                        "No URL yet".to_string()
                    } else {
                        result.url
                    },
                    index == selected_index,
                    target,
                    cx,
                )
            }))
    }

    fn render_global_search_history_group(
        &self,
        results: &[GlobalSearchHistoryResult],
        index_offset: usize,
        selected_index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .debug_selector(|| "global-search-history-group".into())
            .flex()
            .flex_col()
            .child(Self::render_global_search_group_heading(
                "HISTORY",
                results.len(),
            ))
            .children(results.iter().cloned().enumerate().map(|(index, result)| {
                let global_index = index_offset + index;
                let target = GlobalSearchTarget::History(result.entry_id.clone());
                let detail = Self::history_result_detail(&result);
                self.render_global_search_result_row(
                    "global-search-history-result",
                    index,
                    result.method,
                    result.display_name,
                    detail,
                    global_index == selected_index,
                    target,
                    cx,
                )
            }))
    }

    fn render_global_search_group_heading(label: &'static str, count: usize) -> impl IntoElement {
        div()
            .h(px(34.0))
            .flex_none()
            .flex()
            .items_center()
            .px_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL_ALT))
            .font_family(FONT_UI)
            .text_size(px(9.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(MUTED))
            .child(format!("{label}  ·  {count}"))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_global_search_result_row(
        &self,
        selector_prefix: &'static str,
        index: usize,
        method: crate::models::HttpMethod,
        display_name: String,
        detail: String,
        selected: bool,
        target: GlobalSearchTarget,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selector = format!("{selector_prefix}-{index}");
        let accessible_label = format!("{method} {display_name} {detail}");
        div()
            .id((selector_prefix, index))
            .debug_selector(move || selector.clone())
            .role(Role::Button)
            .aria_label(accessible_label)
            .aria_selected(selected)
            .h(px(64.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(if selected { INFO_SOFT } else { PANEL }))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(INFO_SOFT)))
            .when(selected, |row| row.border_l_2().border_color(rgb(INFO)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.execute_global_search_target(target.clone(), window, cx)
                }),
            )
            .child(
                div()
                    .w(px(52.0))
                    .h(px(26.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(rgb(PANEL_ALT))
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(method_color(method)))
                    .child(method.to_string()),
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
                            .overflow_hidden()
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child(display_name),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .font_family(FONT_MONO)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(FONT_UI)
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(if selected { INFO } else { MUTED }))
                    .child(if selected { "Enter" } else { "Open" }),
            )
    }

    fn render_global_search_footer(&self) -> impl IntoElement {
        div()
            .h(px(44.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .bg(rgb(PANEL_ALT))
            .font_family(FONT_UI)
            .text_size(px(9.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child("↑ ↓  Navigate")
            .child("Enter  Open")
            .child("Esc  Close")
    }

    fn render_global_search_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "global-search-empty".into())
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(194.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(48.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(14.0))
                            .bg(rgb(INFO_SOFT))
                            .font_family(FONT_UI)
                            .text_size(px(22.0))
                            .text_color(rgb(INFO))
                            .child("⌕"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("No matching requests"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(10.0))
                            .text_color(rgb(SUBTEXT))
                            .child("Try a URL fragment, request name, or HTTP method."),
                    )
                    .child(
                        div()
                            .id("global-search-empty-clear")
                            .debug_selector(|| "global-search-empty-clear".into())
                            .role(Role::Button)
                            .aria_label("Clear global search")
                            .h(px(30.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .bg(rgb(ACCENT_SOFT))
                            .cursor_pointer()
                            .font_family(FONT_UI)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(ACCENT))
                            .child("×  Clear search")
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.clear_global_search(window, cx)
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(56.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL_ALT))
                    .font_family(FONT_UI)
                    .text_size(px(9.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(MUTED))
                            .child("Esc"),
                    )
                    .child("Close search and restore the previous editor focus"),
            )
    }

    fn history_result_detail(result: &GlobalSearchHistoryResult) -> String {
        let mut parts = vec![result.url.clone()];
        if let Some(status) = result.status {
            parts.push(status.to_string());
        }
        if let Some(size) = result.response_size {
            parts.push(Self::format_global_search_size(size));
        }
        parts.join("  ·  ")
    }

    fn format_global_search_size(size: usize) -> String {
        if size < 1024 {
            format!("{size} B")
        } else {
            format!("{:.1} KB", size as f64 / 1024.0)
        }
    }
}
