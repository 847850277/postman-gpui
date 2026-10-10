use super::*;
use crate::ui::{
    components::kit_controls,
    theme::{metrics, FONT_MONO, FONT_UI},
};
use gpui::{
    div, prelude::FluentBuilder, rems, AnyElement, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    base::{Button, ElementExt},
    component::{
        scroll::{Scrollbar, ScrollbarMode},
        ActiveTheme, Icon,
    },
};

// Prototype dimensions expressed in rem so controls, rows, and the preferred height zoom together.
const ROW: f32 = 46. / 16.;
const NARROW_ROW: f32 = 88. / 16.;
const HEADER: f32 = 30. / 16.;
const ACTION: f32 = 30. / 16.;
const ADD_GAP: f32 = 10. / 16.;
const TYPE_WIDTH: f32 = 76. / 16.;

fn is_narrow(width: Pixels) -> bool {
    width > Pixels::ZERO && width <= px(440.)
}

fn row_height(narrow: bool) -> f32 {
    if narrow {
        NARROW_ROW
    } else {
        ROW
    }
}

fn table_height(rows: usize, narrow: bool) -> f32 {
    row_height(narrow) * rows as f32 + if narrow { 0. } else { HEADER }
}

fn button(id: impl Into<ElementId>, label: impl Into<gpui::SharedString>, cx: &App) -> Button {
    kit_controls::editor_button(id, label, cx)
        .h(rems(ACTION))
        .px_2()
        .text_size(rems(11. / 16.))
        .font_weight(gpui::FontWeight::NORMAL)
        .cursor_default()
}

fn key_lane() -> Div {
    div().min_w_0().flex_1().flex()
}

fn value_lane() -> Div {
    div().min_w_0().flex_1().flex_grow(1.5).flex()
}

impl FormBodyInput {
    /// Content-fit height, including table borders and the separate Add field button.
    pub(in super::super) fn preferred_height(&self, width: Pixels, cx: &App) -> Pixels {
        cx.theme().font_size
            * (table_height(self.form_data_entries.len(), is_narrow(width)) + ADD_GAP + ACTION)
            + px(2.)
    }

    fn render_header(&self, cx: &App) -> impl IntoElement {
        div()
            .debug_selector(|| "body-form-table-header".into())
            .h(rems(HEADER))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .bg(cx.theme().muted)
            .text_size(metrics::CAPTION)
            .text_color(cx.theme().muted_foreground)
            .child(div().w_8().flex_none())
            .child(key_lane().child(div().px_3().child("Key")))
            .when(self.form_data_allows_files, |header| {
                header.child(div().w(rems(TYPE_WIDTH)).flex_none().px_2().child("Type"))
            })
            .child(value_lane().child(div().px_3().child("Value")))
            .child(div().w_8().flex_none())
    }

    fn render_key(&self, index: usize, narrow: bool, cx: &App) -> impl IntoElement {
        key_lane()
            .debug_selector(move || format!("body-form-key-{index}"))
            .h_8()
            .items_center()
            .when(!self.form_data_entries[index].enabled, |cell| {
                cell.opacity(0.55)
            })
            .when(narrow, |cell| cell.child(self.render_label("Key", cx)))
            .child(self.row_editors[index].key_input.clone())
    }

    fn render_label(&self, label: &'static str, cx: &App) -> impl IntoElement {
        div()
            .w_8()
            .flex_none()
            .text_size(metrics::CAPTION)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    fn render_value(&self, index: usize, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let entry = &self.form_data_entries[index];
        let editor = &self.row_editors[index];
        let row_id = editor.row_id;
        let stable_id = editor.key_input.entity_id();
        value_lane()
            .id(("body-form-value", stable_id))
            .debug_selector(move || format!("body-form-value-{index}"))
            .h_8()
            .items_center()
            .when(!entry.enabled, |cell| cell.opacity(0.55))
            .when(narrow, |cell| cell.child(self.render_label("Value", cx)))
            .child(if let Some(file) = &entry.file {
                let has_file = !file.path.as_os_str().is_empty();
                let name = file.file_name.clone().or_else(|| {
                    file.path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                });
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(has_file, |cell| {
                        cell.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            format!("body-form-file-name-{index}")
                                        })
                                        .truncate()
                                        .font_family(FONT_MONO)
                                        .text_size(rems(11. / 16.))
                                        .child(
                                            name.unwrap_or_else(|| file.path.display().to_string()),
                                        ),
                                )
                                .when_some(file.content_type.clone(), |name, content_type| {
                                    name.child(
                                        div()
                                            .debug_selector(move || {
                                                format!("body-form-file-metadata-{index}")
                                            })
                                            .truncate()
                                            .text_size(rems(9. / 16.))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(content_type),
                                    )
                                }),
                        )
                    })
                    .child(
                        button(
                            ("body-form-file", stable_id),
                            if has_file {
                                "Change…"
                            } else {
                                "Choose file…"
                            },
                            cx,
                        )
                        .debug_selector(move || format!("body-form-file-{index}"))
                        .accessibility_label(format!(
                            "{} file for form row {}",
                            if has_file { "Change" } else { "Choose" },
                            index + 1
                        ))
                        .track_focus(&self.row_file_focus_handles[index])
                        .flex_none()
                        .border_1()
                        .border_color(cx.theme().input)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.choose_form_data_file(row_id, window, cx);
                            },
                        )),
                    )
                    .into_any_element()
            } else {
                editor.value_input.clone().into_any_element()
            })
            .into_any_element()
    }

    fn render_row(&self, index: usize, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = &self.form_data_entries[index];
        let row_id = self.row_editors[index].row_id;
        let stable_id = self.row_editors[index].key_input.entity_id();
        let on_toggle = cx.listener(move |this: &mut Self, _: &gpui::ClickEvent, _, cx| {
            if let Some(index) = this.entry_index(row_id) {
                this.toggle_form_data_entry(index, cx);
            }
        });
        let toggle = div()
            .w(if narrow { rems(1.5) } else { rems(2.) })
            .h_8()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(
                kit_controls::parameter_checkbox(
                    ("body-form-toggle", stable_id),
                    entry.enabled,
                    cx,
                )
                .debug_selector(move || format!("body-form-toggle-{index}"))
                .accessibility_label(format!("Enable form body row {}", index + 1))
                .track_focus(&self.row_toggle_focus_handles[index])
                .cursor_default()
                .on_change(move |_, event, window, cx| on_toggle(event, window, cx)),
            );
        let delete = div()
            .w(if narrow { rems(1.75) } else { rems(2.) })
            .flex_none()
            .flex()
            .justify_center()
            .child(
                // editor_button already owns a hover style; this variant owns its danger hover.
                Button::new(("body-form-delete", stable_id))
                    .h(rems(ACTION))
                    .rounded(metrics::RADIUS)
                    .text_color(cx.theme().muted_foreground)
                    .cursor_default()
                    .focus_visible(|style| style.border_1().border_color(cx.theme().ring))
                    .debug_selector(move || format!("body-form-delete-{index}"))
                    .accessibility_label(format!("Remove form body row {}", index + 1))
                    .track_focus(&self.row_delete_focus_handles[index])
                    .w_7()
                    .p_0()
                    .child(Icon::new(IconName::Trash).size(metrics::SMALL_ICON))
                    .hover(|style| {
                        style
                            .bg(cx.theme().danger.opacity(0.1))
                            .text_color(cx.theme().danger)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(index) = this.entry_index(row_id) {
                            this.remove_form_data_entry(index, cx);
                            this.focus_after_row_removal(index, window, cx);
                        }
                    })),
            );
        let line = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .h_8()
            .flex_none()
            .child(toggle)
            .child(self.render_key(index, narrow, cx))
            .when(self.form_data_allows_files, |line| {
                line.child(
                    button(
                        ("body-form-type", stable_id),
                        if entry.file.is_some() { "File" } else { "Text" },
                        cx,
                    )
                    .debug_selector(move || format!("body-form-type-{index}"))
                    .accessibility_label(format!(
                        "Use {} value for form row {}",
                        if entry.file.is_some() { "text" } else { "file" },
                        index + 1
                    ))
                    .track_focus(&self.row_type_focus_handles[index])
                    .w(rems(TYPE_WIDTH))
                    .flex_none()
                    .border_1()
                    .border_color(cx.theme().input)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(index) = this.entry_index(row_id) {
                            this.toggle_form_data_value_kind(index, cx);
                        }
                    })),
                )
            });
        let value = self.render_value(index, narrow, cx);
        let row = div()
            .id(("body-form-row", stable_id))
            .debug_selector(move || format!("body-form-row-{index}"))
            .h(rems(row_height(narrow)))
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .px_2()
            .when(!narrow || index > 0, |row| {
                row.border_t_1().border_color(cx.theme().border)
            });
        if narrow {
            row.gap_2().child(line.child(delete)).child(
                div()
                    .h_8()
                    .flex_none()
                    .flex()
                    .gap_2()
                    .child(div().w_6().flex_none())
                    .child(value)
                    .child(div().w_7().flex_none()),
            )
        } else {
            row.child(line.child(value).child(delete))
        }
    }

    // Native Tab through Kit controls and cell traversal share the same scroll owner.
    // Only a changed focus target scrolls, so wheel scrolling does not snap back to the editor.
    fn reveal_focused_row(&mut self, window: &Window, cx: &App) {
        let focused = self
            .row_editors
            .iter()
            .enumerate()
            .find(|(index, row)| {
                row.key_input.read(cx).focus_handle(cx).is_focused(window)
                    || row.value_input.read(cx).focus_handle(cx).is_focused(window)
                    || self.row_toggle_focus_handles[*index].is_focused(window)
                    || self.row_type_focus_handles[*index].is_focused(window)
                    || self.row_file_focus_handles[*index].is_focused(window)
                    || self.row_delete_focus_handles[*index].is_focused(window)
            })
            .map(|(index, row)| (index, row.row_id));
        let row_id = focused.map(|(_, row_id)| row_id);
        if self.focused_row != row_id {
            self.focused_row = row_id;
            if let Some((index, _)) = focused {
                self.form_data_scroll.scroll_to_item(index);
            }
        }
    }
}

impl Render for FormBodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_focus(window, cx);
        self.reveal_focused_row(window, cx);
        let narrow = is_narrow(self.viewport_width);
        let rows = (0..self.form_data_entries.len())
            .map(|index| self.render_row(index, narrow, cx).into_any_element())
            .collect::<Vec<_>>();
        let scroll = div()
            .id("body-form-scroll")
            .debug_selector(|| "body-form-scroll".into())
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.form_data_scroll)
            .children(rows)
            .on_prepaint({
                let this = cx.weak_entity();
                let scroll = self.form_data_scroll.clone();
                let previous = self.has_overflow;
                move |_, window, cx| {
                    let has_overflow = scroll.max_offset().y > Pixels::ZERO;
                    if has_overflow != previous {
                        window.defer(cx, move |_, cx| {
                            let _ = this.update(cx, |this, cx| {
                                if this.has_overflow != has_overflow {
                                    this.has_overflow = has_overflow;
                                    cx.notify();
                                }
                            });
                        });
                    }
                }
            });
        div()
            .debug_selector(|| "body-form-editor".into())
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(rems(ADD_GAP))
            .font_family(FONT_UI)
            .text_color(cx.theme().foreground)
            .on_prepaint({
                let this = cx.weak_entity();
                let previous = self.viewport_width;
                move |bounds, window, cx| {
                    let width = bounds.size.width;
                    if width != previous {
                        window.defer(cx, move |_, cx| {
                            let _ = this.update(cx, |this, cx| {
                                if this.viewport_width != width {
                                    if is_narrow(this.viewport_width) != is_narrow(width) {
                                        this.focused_row = None;
                                    }
                                    this.viewport_width = width;
                                    cx.notify();
                                }
                            });
                        });
                    }
                }
            })
            .child(
                div()
                    .debug_selector(|| "body-form-table".into())
                    .h(rems(table_height(self.form_data_entries.len(), narrow))
                        .to_pixels(window.rem_size())
                        + px(2.))
                    .flex_shrink_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(metrics::RADIUS)
                    .bg(cx.theme().background)
                    .overflow_hidden()
                    .when(!narrow, |table| table.child(self.render_header(cx)))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .flex()
                            .child(scroll)
                            .when(self.has_overflow, |viewport| {
                                viewport.child(
                                    div()
                                        .debug_selector(|| "body-form-scrollbar".into())
                                        .absolute()
                                        .top_0()
                                        .right_0()
                                        .bottom_0()
                                        .w_2()
                                        .child(
                                            Scrollbar::vertical(&self.form_data_scroll)
                                                .id("body-form-scrollbar-control")
                                                .mode(ScrollbarMode::Always),
                                        ),
                                )
                            }),
                    ),
            )
            .child(
                div().flex_none().flex().items_start().child(
                    button("body-form-add-row", "", cx)
                        .debug_selector(|| "body-form-add-row".into())
                        .accessibility_label("Add form body row")
                        .track_focus(&self.add_row_focus_handle)
                        .gap_1()
                        .child(Icon::new(IconName::Plus).size(metrics::SMALL_ICON))
                        .child("Add field")
                        .on_click(cx.listener(|this, _, _, cx| this.add_form_data_entry(cx))),
                ),
            )
    }
}
