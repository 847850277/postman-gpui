//! Shared endpoint settings. Kit owns selection, editing, overlays, and focus; this feature owns
//! validation and the commit boundary. Requests change only after a durable commit succeeds.
mod dialog;
mod model;
mod storage;

use crate::ui::theme::{metrics as m, ERROR};
use dialog::EnvironmentEditor;
use gpui::InteractiveElement;
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, EventEmitter, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Task, Window,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        searchable_list::{SearchableListItem, SearchableVec},
        select::{Select, SelectEvent, SelectState},
        Disableable, Sizable, WindowExt,
    },
};
pub(crate) use model::rebase_url;
use model::EnvironmentSet;
use std::path::PathBuf;

#[derive(Clone)]
struct EnvironmentOption {
    id: Option<String>,
    label: SharedString,
}
impl SearchableListItem for EnvironmentOption {
    type Value = Option<String>;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.id
    }
}

pub(crate) struct EnvironmentChanged {
    pub(crate) previous_base: Option<String>,
    pub(crate) next_base: Option<String>,
}

pub(crate) struct Environments {
    path: Option<PathBuf>,
    current: EnvironmentSet,
    selector: Entity<SelectState<SearchableVec<EnvironmentOption>>>,
    editor: Option<Entity<EnvironmentEditor>>,
    loading: bool,
    saving: bool,
    blocked: bool,
    error: Option<String>,
    task: Option<Task<()>>,
    _subscription: Subscription,
}
impl EventEmitter<EnvironmentChanged> for Environments {}

impl Environments {
    pub(crate) fn active_base(&self) -> Option<&str> {
        self.current.active_base()
    }
    pub(crate) fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selector = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<EnvironmentOption>::new()),
                None,
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(&selector, window, |this, _, event, w, cx| {
            let SelectEvent::Confirm(Some(id)) = event else {
                return;
            };
            let id = id.clone();
            // The component reports intent. Restore the committed value until the write succeeds.
            this.sync_selector(w, cx);
            if this.loading || this.saving {
                return;
            }
            match id {
                None => this.open(w, cx),
                Some(id) if !this.blocked && this.current.active_id.as_ref() != Some(&id) => {
                    let mut next = this.current.clone();
                    next.active_id = Some(id);
                    this.save(next, false, w, cx);
                }
                _ => {}
            }
        });
        let mut this = Self {
            path,
            current: EnvironmentSet::default(),
            selector,
            editor: None,
            loading: false,
            saving: false,
            blocked: true,
            error: None,
            task: None,
            _subscription: subscription,
        };
        this.reload(window, cx);
        this
    }

    fn sync_selector(&self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items: Vec<_> = self
            .current
            .environments
            .iter()
            .map(|e| EnvironmentOption {
                id: Some(e.id.clone()),
                label: e.name.clone().into(),
            })
            .collect();
        items.push(EnvironmentOption {
            id: None,
            label: "Manage environments…".into(),
        });
        self.selector.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(items), window, cx);
            if let Some(id) = &self.current.active_id {
                state.set_selected_value(&Some(id.clone()), window, cx);
            } else {
                state.set_selected_index(None, window, cx);
            }
            cx.notify();
        });
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.saving {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.error = Some(
                "The local configuration directory is unavailable. Restart after restoring access."
                    .into(),
            );
            return;
        };
        self.loading = true;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let read_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { storage::load(&read_path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok((current, warning)) => {
                        this.current = current;
                        this.error = warning;
                        this.blocked = false;
                    }
                    Err(error) => {
                        this.error = Some(format!(
                            "{error} Restore or repair {} and reload. The file has not been overwritten.",
                            path.display()
                        ));
                        this.blocked = true;
                    }
                }
                this.sync_selector(window, cx);
                if let Some(editor) = &this.editor {
                    editor.update(cx, |editor, cx| {
                        editor.reset(&this.current, window, cx);
                        if !this.blocked {
                            editor.focus_first(window, cx);
                        }
                    });
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn save(
        &mut self,
        next: EnvironmentSet,
        close_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.saving || self.blocked {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        self.saving = true;
        self.error = None;
        let write = next.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { storage::save(&path, &write) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => {
                        let previous_base = this.current.active_base().map(str::to_string);
                        this.current = next;
                        this.sync_selector(window, cx);
                        cx.emit(EnvironmentChanged {
                            previous_base,
                            next_base: this.current.active_base().map(str::to_string),
                        });
                        if close_editor {
                            window.close_dialog(cx);
                        }
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) || self.saving || self.loading {
            return;
        }
        let owner = cx.weak_entity();
        let editor = cx.new(|cx| EnvironmentEditor::new(owner, &self.current, window, cx));
        self.editor = Some(editor.clone());
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            let saving = owner
                .read_with(cx, |owner, _| owner.saving)
                .unwrap_or(false);
            let width = (window.rem_size() * 37.5)
                .min(window.viewport_size().width - window.rem_size() * 2.);
            let height = (window.rem_size() * 36.)
                .min(window.viewport_size().height - window.rem_size() * 2.);
            let content = editor.clone();
            let close_owner = owner.clone();
            let submit = editor.downgrade();
            dialog
                .w(width)
                .h(height)
                .margin_top((window.viewport_size().height - height) / 2.)
                .p_0()
                .gap_0()
                .close_button(false)
                .keyboard(!saving)
                .overlay_closable(!saving)
                .content(move |c, _, _| c.p_0().min_h_0().child(content.clone()))
                .on_close(move |_, _, cx| {
                    let _ = close_owner.update(cx, |owner, cx| {
                        owner.editor = None;
                        cx.notify();
                    });
                })
                .on_ok(move |_, window, cx| {
                    let _ = submit.update(cx, |editor, cx| editor.submit(window, cx));
                    false
                })
        });
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| editor.focus_first(window, cx));
        }
        cx.notify();
    }
}

impl Render for Environments {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .flex_none()
            .child(
                div()
                    .debug_selector(|| "environment-selector".into())
                    .child(
                        Select::new(&self.selector)
                            .id("environment-select")
                            .accessibility_label("Environment")
                            .placeholder(if self.loading {
                                "Loading environments…"
                            } else {
                                "No environment"
                            })
                            .small()
                            .w(gpui::rems(11.))
                            .menu_width(gpui::rems(17.))
                            .disabled(self.loading || self.saving || self.blocked),
                    ),
            )
            .child(
                Button::new("environment-settings")
                    .debug_selector(|| "environment-settings".into())
                    .label(if self.current.environments.is_empty() && !self.blocked {
                        "Create environment…"
                    } else {
                        "Settings…"
                    })
                    .icon(IconName::Settings)
                    .small()
                    .disabled(self.loading || self.saving)
                    .on_click(cx.listener(|this, _, w, cx| this.open(w, cx))),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    Button::new("environment-error")
                        .debug_selector(|| "environment-error".into())
                        .ghost()
                        .icon(IconName::CircleAlert)
                        .accessibility_label("Environment settings need attention")
                        .tooltip(error)
                        .text_color(ERROR.resolve(cx))
                        .small()
                        .disabled(self.loading || self.saving)
                        .on_click(cx.listener(|this, _, w, cx| this.open(w, cx))),
                )
            })
            .text_size(m::LABEL)
    }
}
