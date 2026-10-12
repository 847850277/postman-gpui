use super::{
    model::{Environment, EnvironmentSet, FieldError},
    Environments,
};
use crate::ui::theme::{metrics as m, ERROR, FONT_MONO, LINE, MUTED, OK, PANEL, PANEL_ALT, TEXT};
use gpui::{
    div, prelude::FluentBuilder, App, AppContext, Context, Entity, IntoElement, ParentElement,
    Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
};
use gpui::{Focusable, InteractiveElement};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputGroup, InputState},
        scroll::ScrollableElement,
        Disableable, FocusableExt, Sizable, WindowExt,
    },
};

struct EnvironmentRow {
    id: String,
    name: Entity<InputState>,
    base_url: Entity<InputState>,
}

pub(super) struct EnvironmentEditor {
    owner: WeakEntity<Environments>,
    rows: Vec<EnvironmentRow>,
    initial_active_id: Option<String>,
    error: Option<FieldError>,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl EnvironmentEditor {
    pub(super) fn new(
        owner: WeakEntity<Environments>,
        value: &EnvironmentSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(
            &owner.upgrade().expect("environment owner exists"),
            |_, _, cx| cx.notify(),
        );
        let mut this = Self {
            owner,
            rows: Vec::new(),
            initial_active_id: None,
            error: None,
            scroll: ScrollHandle::new(),
            _subscription: subscription,
        };
        this.reset(value, window, cx);
        this
    }

    pub(super) fn reset(
        &mut self,
        value: &EnvironmentSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.initial_active_id = value.active_id.clone();
        self.rows = value
            .environments
            .iter()
            .cloned()
            .map(|env| Self::row(env, window, cx))
            .collect();
        if self.rows.is_empty() {
            self.rows.push(Self::row(Environment::blank(), window, cx));
        }
        self.error = None;
        cx.notify();
    }

    fn row(env: Environment, window: &mut Window, cx: &mut Context<Self>) -> EnvironmentRow {
        EnvironmentRow {
            id: env.id,
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("e.g. Staging")
                    .default_value(env.name)
            }),
            base_url: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://api.example.com")
                    .default_value(env.base_url)
            }),
        }
    }

    pub(super) fn focus_first(&self, window: &mut Window, cx: &mut App) {
        let ix = self
            .rows
            .iter()
            .position(|r| Some(&r.id) == self.initial_active_id.as_ref())
            .unwrap_or(0);
        if let Some(row) = self.rows.get(ix) {
            row.name.read(cx).focus_handle(cx).focus(window, cx);
            // The first layout establishes ScrollHandle's viewport. The help and feedback
            // precede the cards; the scrollbar stays after them so these indices remain stable.
            let scroll = self.scroll.clone();
            window.on_next_frame(move |window, _| {
                scroll.scroll_to_item(ix + 2);
                window.refresh();
            });
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = Self::row(Environment::blank(), window, cx);
        row.name.read(cx).focus_handle(cx).focus(window, cx);
        self.rows.push(row);
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.saving || state.loading || state.blocked {
            return;
        }
        let active = state.current.active_id.clone();
        let rows = self
            .rows
            .iter()
            .map(|row| Environment {
                id: row.id.clone(),
                name: row.name.read(cx).value().to_string(),
                base_url: row.base_url.read(cx).value().to_string(),
            })
            .collect();
        match EnvironmentSet::draft(rows, active).validate() {
            Ok(value) => {
                self.error = None;
                owner.update(cx, |owner, cx| owner.save(value, true, window, cx));
            }
            Err(error) => {
                if let Some((ix, row)) = self
                    .rows
                    .iter()
                    .enumerate()
                    .find(|(_, row)| row.id == error.id)
                {
                    self.scroll.scroll_to_item(ix + 2);
                    let input = if error.field == "name" {
                        &row.name
                    } else {
                        &row.base_url
                    };
                    input.read(cx).focus_handle(cx).focus(window, cx);
                }
                self.error = Some(error);
            }
        }
        cx.notify();
    }

    fn field(
        &self,
        row: &EnvironmentRow,
        field: &'static str,
        label: &'static str,
        disabled: bool,
        cx: &App,
    ) -> impl IntoElement {
        let id = format!("environment-{}-{field}", row.id);
        let selector = id.clone();
        let error = self
            .error
            .as_ref()
            .filter(|e| e.id == row.id && e.field == field);
        let input = if field == "name" {
            &row.name
        } else {
            &row.base_url
        };
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_2()
            .flex_1()
            .child(
                div()
                    .text_size(m::COMPACT_LABEL)
                    .text_color(MUTED.resolve(cx))
                    .child(label),
            )
            .child(
                div().debug_selector(move || selector.clone()).child(
                    InputGroup::new(format!("{id}-group"))
                        .aria_label(label)
                        .h(m::CONTROL)
                        .focus_ring(false)
                        .bg(PANEL.resolve(cx))
                        .disabled(disabled)
                        .invalid(error.is_some())
                        .input(
                            Input::new(input)
                                .aria_label(label)
                                .id(id)
                                .text_size(m::LABEL)
                                .when(field == "base-url", |i| {
                                    i.font_family(FONT_MONO).text_size(m::COMPACT_LABEL)
                                }),
                        ),
                ),
            )
            .when_some(error, |d, error| {
                d.child(
                    div()
                        .text_size(m::LABEL)
                        .text_color(ERROR.resolve(cx))
                        .child(error.message.clone()),
                )
            })
    }
}

impl EnvironmentEditor {
    fn render_header(&self, saving: bool, cx: &App) -> impl IntoElement {
        div()
            .debug_selector(|| "environment-dialog-header".into())
            .flex_none()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .flex()
            .items_center()
            .gap_3()
            .child(gpui_kit::component::Icon::new(IconName::Settings).size(m::ICON))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(m::BODY)
                            .font_weight(m::SEMIBOLD)
                            .child("Environments"),
                    )
                    .child(
                        div()
                            .text_size(m::LABEL)
                            .text_color(MUTED.resolve(cx))
                            .child("Shared across HTTP and Flows"),
                    ),
            )
            .child(
                Button::new("environment-close")
                    .debug_selector(|| "environment-close".into())
                    .ghost()
                    .icon(IconName::X)
                    .small()
                    .accessibility_label("Close environments")
                    .disabled(saving)
                    .on_click(|_, w, cx| w.close_dialog(cx)),
            )
    }

    fn render_card(
        &self,
        ix: usize,
        row: &EnvironmentRow,
        active: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let remove = row.id.clone();
        let remove_id = format!("environment-remove-{}", row.id);
        let selector = remove_id.clone();
        div()
            .id(gpui::SharedString::from(format!(
                "environment-card-{}",
                row.id
            )))
            .flex_none()
            .mx_5()
            .p_4()
            .rounded(m::RADIUS)
            .border_1()
            .border_color(LINE.resolve(cx))
            .bg(PANEL_ALT.resolve(cx))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(m::LABEL)
                            .font_weight(m::MEDIUM)
                            .child(format!("Environment {:02}", ix + 1)),
                    )
                    .when(active, |d| {
                        d.child(
                            div()
                                .text_size(m::CAPTION)
                                .text_color(OK.resolve(cx))
                                .bg(OK.resolve(cx).opacity(0.1))
                                .px(gpui::rems(6. / 16.))
                                .py(gpui::rems(2. / 16.))
                                .rounded_sm()
                                .child("Active"),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new(remove_id)
                            .debug_selector(move || selector.clone())
                            .ghost()
                            .icon(IconName::X)
                            .small()
                            .accessibility_label("Remove environment")
                            .disabled(disabled || self.rows.len() == 1)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if this.rows.len() <= 1 {
                                    return;
                                }
                                this.rows.retain(|r| r.id != remove);
                                this.error = None;
                                this.focus_first(w, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.field(row, "name", "Name", disabled, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex_grow(1.7)
                            .child(self.field(row, "base-url", "Base URL", disabled, cx)),
                    ),
            )
            .child(
                div()
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child("Paths and query parameters are preserved."),
            )
    }

    fn render_footer(
        &self,
        saving: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .debug_selector(|| "environment-dialog-footer".into())
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
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child("Changes apply when saved"),
            )
            .child(div().flex_1())
            .child(
                Button::new("environment-cancel")
                    .debug_selector(|| "environment-cancel".into())
                    .label("Cancel")
                    .xsmall()
                    .h(m::CONTROL)
                    .disabled(saving)
                    .on_click(|_, w, cx| w.close_dialog(cx)),
            )
            .child(
                Button::new("environment-save")
                    .debug_selector(|| "environment-save".into())
                    .primary()
                    .label(if saving { "Saving…" } else { "Save changes" })
                    .xsmall()
                    .h(m::CONTROL)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, w, cx| this.submit(w, cx))),
            )
    }
}

impl Render for EnvironmentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (saving, loading, blocked, active, error) = self
            .owner
            .read_with(cx, |o, _| {
                (
                    o.saving,
                    o.loading,
                    o.blocked,
                    o.current.active_id.clone(),
                    o.error.clone(),
                )
            })
            .unwrap_or((false, false, true, None, None));
        let disabled = saving || loading || blocked;
        let feedback = div()
            .flex_none()
            .mx_5()
            .flex()
            .flex_col()
            .gap_2()
            .when(error.is_none() && !blocked, |d| d.hidden())
            .when_some(error, |d, error| {
                d.child(
                    div()
                        .debug_selector(|| "environment-save-error".into())
                        .text_size(m::LABEL)
                        .text_color(ERROR.resolve(cx))
                        .child(error),
                )
            })
            .when(blocked, |d| {
                d.child(
                    Button::new("environment-reload")
                        .debug_selector(|| "environment-reload".into())
                        .label("Reload configuration")
                        .small()
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, w, cx| {
                            let _ = this.owner.update(cx, |owner, cx| owner.reload(w, cx));
                        })),
                )
            });
        let cards: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| {
                self.render_card(ix, row, active.as_ref() == Some(&row.id), disabled, cx)
                    .into_any_element()
            })
            .collect();
        let help = div()
            .flex_none()
            .mx_5()
            .text_size(m::LABEL)
            .text_color(MUTED.resolve(cx))
            .child("Choose a base URL for each environment. Save to apply changes to matching open requests.");
        let add = div().flex_none().mx_5().flex().child(
            Button::new("environment-add")
                .debug_selector(|| "environment-add".into())
                .ghost()
                .icon(IconName::Plus)
                .label("Add environment")
                .xsmall()
                .h_7()
                .disabled(disabled)
                .on_click(cx.listener(|this, _, w, cx| this.add(w, cx))),
        );
        div()
            .debug_selector(|| "environment-dialog".into())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .rounded(m::DIALOG_RADIUS)
            .child(self.render_header(saving, cx))
            .child(
                div()
                    .id("environment-content")
                    .debug_selector(|| "environment-content".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .py_5()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(help)
                    .child(feedback)
                    .children(cards)
                    .child(add)
                    .vertical_scrollbar(&self.scroll),
            )
            .child(self.render_footer(saving, disabled, cx))
    }
}
