//! The Flow feature owns transfer decisions. The shell supplies the shared HTTP model;
//! neither editor reaches into the other editor's controls or changes its active tab.
use super::{model::TransferDestination, FlowId, FlowsEvent, FlowsView};
use crate::{
    app::{RequestTabId, RequestViewModel, WorkspaceViewModel},
    models::flow_request,
    ui::theme::{metrics as m, ERROR, MUTED, PANEL_ALT},
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        dialog::Cancel,
        input::{Input, InputEvent, InputState},
        searchable_list::{SearchableListItem, SearchableVec},
        select::{Select, SelectEvent, SelectState},
        Disableable, IndexPath, Sizable, WindowExt,
    },
};
use gpui_kit::{
    div, prelude::FluentBuilder, App, AppContext, Context, Entity, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, WeakEntity, Window,
};
use postman_flow::HttpRequestTemplate;

#[derive(Clone, PartialEq)]
enum DestinationChoice {
    Existing(FlowId),
    New,
}
#[derive(Clone)]
struct Choice<T: Clone> {
    id: T,
    label: gpui_kit::SharedString,
}
impl<T: Clone + PartialEq + 'static> SearchableListItem for Choice<T> {
    type Value = T;
    fn title(&self) -> gpui_kit::SharedString {
        self.label.clone()
    }
    fn value(&self) -> &T {
        &self.id
    }
}
type Picker<T> = Entity<SelectState<SearchableVec<Choice<T>>>>;

// Never put URL userinfo, query values, headers or body contents in a summary.
fn source_name(request: &RequestViewModel) -> String {
    if request.saved_document_id().is_some() {
        return request.tab_title();
    }
    let host = reqwest::Url::parse(request.url())
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    format!("Request to {}", host.as_deref().unwrap_or("HTTP"))
}
struct Snapshot {
    tab: RequestTabId,
    revision: u64,
    method: crate::models::HttpMethod,
    name: String,
    request: Result<HttpRequestTemplate, flow_request::ConversionError>,
}
impl Snapshot {
    fn capture(source: &RequestViewModel) -> Self {
        Self {
            tab: source.tab_id(),
            revision: source.draft_revision(),
            method: source.method(),
            name: source_name(source),
            request: flow_request::convert(
                source.request_draft(),
                source.pre_request_script(),
                source.tests_script(),
            ),
        }
    }
    fn validate(&self, workspace: &WorkspaceViewModel) -> Result<(), &'static str> {
        let source = workspace
            .request_for_tab(self.tab)
            .ok_or("This HTTP tab was closed. Cancel and choose an open request.")?;
        if source.draft_revision() != self.revision || source_name(source) != self.name {
            return Err("This HTTP request changed. Cancel and reopen Add to flow to copy its latest draft.");
        }
        self.request.as_ref().map(|_| ()).map_err(|e| e.message())
    }
}

pub(in crate::app::postman_app) fn open(
    flows: Entity<FlowsView>,
    workspace: Entity<WorkspaceViewModel>,
    source: Option<RequestTabId>,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }
    let target = if source.is_none() {
        flows.read(cx).library.active().map(|d| d.id())
    } else {
        None
    };
    if source.is_none() && target.is_none() {
        return;
    }
    let trigger = window.focused(cx);
    let dialog = cx.new(|cx| TransferDialog::new(flows, workspace, source, target, window, cx));
    let view = dialog.clone();
    window.open_dialog(cx, move |d, w, cx| {
        let view = view.clone();
        let submit = view.clone();
        let trigger = trigger.clone();
        let state = view.read(cx);
        let disabled = state.error(cx).is_some() || state.submitted;
        d.title(if target.is_some() {
            "Add a step"
        } else {
            "Add to flow"
        })
        .w((w.rem_size() * 34.).min(w.viewport_size().width - w.rem_size() * 2.))
        .margin_top((w.viewport_size().height - w.rem_size() * 24.).max(w.rem_size() * 2.) / 2.)
        .child(view)
        .footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("flow-transfer-cancel")
                        .label("Cancel")
                        .on_click(|_, w, cx| {
                            w.dispatch_action(Box::new(Cancel), cx);
                        }),
                )
                .child(
                    Button::new("flow-transfer-submit")
                        .primary()
                        .label("Add and open flow")
                        .disabled(disabled)
                        .on_click(move |_, w, cx| submit.update(cx, |s, cx| s.submit(w, cx))),
                ),
        )
        .on_close(move |_, w, cx| {
            if let Some(trigger) = &trigger {
                trigger.focus(w, cx);
            }
        })
        // Enter belongs to the focused Kit control; a stray Enter must not choose a source.
        .on_ok(|_, _, cx| {
            cx.propagate();
            false
        })
    });
    dialog.update(cx, |d, cx| {
        if target.is_some() && !d.has_sources {
            d.blank_focus.focus(window, cx);
        } else if target.is_some() {
            d.source.focus_handle(cx).focus(window, cx);
        } else {
            d.destination.focus_handle(cx).focus(window, cx);
        }
    });
}

struct TransferDialog {
    flows: WeakEntity<FlowsView>,
    workspace: Entity<WorkspaceViewModel>,
    source: Picker<RequestTabId>,
    destination: Picker<DestinationChoice>,
    name: Entity<InputState>,
    snapshot: Option<Snapshot>,
    target: Option<FlowId>,
    has_sources: bool,
    blank_focus: gpui_kit::FocusHandle,
    submitted: bool,
    failure: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}
impl TransferDialog {
    fn new(
        flows: Entity<FlowsView>,
        workspace: Entity<WorkspaceViewModel>,
        tab: Option<RequestTabId>,
        target: Option<FlowId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = tab
            .and_then(|id| workspace.read(cx).request_for_tab(id))
            .map(Snapshot::capture);
        let sources: Vec<_> = workspace
            .read(cx)
            .tabs()
            .iter()
            .map(|r| Choice {
                id: r.tab_id(),
                label: format!("{} · {} · Tab {}", r.method(), source_name(r), r.tab_id().0).into(),
            })
            .collect();
        let has_sources = !sources.is_empty();
        let source = cx.new(|cx| {
            SelectState::new(SearchableVec::new(sources), None, window, cx).searchable(true)
        });
        let mut choices: Vec<_> = flows
            .read(cx)
            .library
            .drafts()
            .iter()
            .map(|d| Choice {
                id: DestinationChoice::Existing(d.id()),
                label: format!("{} · #{}", d.title(), d.id().0).into(),
            })
            .collect();
        let initial = flows
            .read(cx)
            .library
            .active()
            .and_then(|d| {
                choices
                    .iter()
                    .position(|c| c.id == DestinationChoice::Existing(d.id()))
            })
            .unwrap_or(0);
        choices.push(Choice {
            id: DestinationChoice::New,
            label: "Create a new flow…".into(),
        });
        let destination = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(choices),
                Some(IndexPath::new(initial)),
                window,
                cx,
            )
            .searchable(true)
        });
        let default_name = snapshot
            .as_ref()
            .map(|s| format!("{} flow", s.name.chars().take(75).collect::<String>()))
            .unwrap_or_else(|| "HTTP flow".into());
        let name = cx.new(|cx| InputState::new(window, cx).default_value(default_name));
        let subscriptions = vec![
            cx.subscribe(&source, |this, _, event, cx| {
                let SelectEvent::Confirm(id) = event;
                this.snapshot = id
                    .and_then(|id| this.workspace.read(cx).request_for_tab(id))
                    .map(Snapshot::capture);
                this.failure = None;
                cx.notify();
            }),
            cx.subscribe(
                &destination,
                |this, _, _: &SelectEvent<SearchableVec<Choice<DestinationChoice>>>, cx| {
                    this.failure = None;
                    cx.notify();
                },
            ),
            cx.subscribe_in(&name, window, |this, _, event, w, cx| {
                this.failure = None;
                if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                    this.submit(w, cx);
                }
                cx.notify();
            }),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&flows, |_, _, cx| cx.notify()),
        ];
        Self {
            flows: flows.downgrade(),
            workspace,
            source,
            destination,
            name,
            snapshot,
            target,
            has_sources,
            blank_focus: cx.focus_handle().tab_stop(true),
            submitted: false,
            failure: None,
            _subscriptions: subscriptions,
        }
    }
    fn destination(&self, cx: &App) -> TransferDestination {
        match self
            .target
            .map(DestinationChoice::Existing)
            .or_else(|| self.destination.read(cx).selected_value().cloned())
        {
            Some(DestinationChoice::Existing(id)) => TransferDestination::Existing(id),
            _ => TransferDestination::New(self.name.read(cx).value().to_string()),
        }
    }
    fn error(&self, cx: &App) -> Option<&'static str> {
        if let Some(error) = self.failure {
            return Some(error);
        }
        let Some(snapshot) = &self.snapshot else {
            return Some("Choose an open HTTP request.");
        };
        if let Err(error) = snapshot.validate(self.workspace.read(cx)) {
            return Some(error);
        }
        self.flows
            .read_with(cx, |v, _| {
                v.library.validate_destination(&self.destination(cx))
            })
            .unwrap_or(Err("The Flow editor is no longer available."))
            .err()
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitted || self.error(cx).is_some() {
            return;
        }
        let snapshot = self.snapshot.as_ref().expect("validated snapshot");
        let request = snapshot
            .request
            .as_ref()
            .expect("validated request")
            .clone();
        let name = snapshot.name.clone();
        let destination = self.destination(cx);
        let result = self.flows.update(cx, |view, cx| {
            view.library
                .append_request(&destination, Some(name), request)?;
            // Close first: Kit restores the trigger, then the new inspector takes focus.
            window.close_dialog(cx);
            view.finish_append(window, cx);
            cx.emit(FlowsEvent::RequestAdded);
            Ok::<_, &'static str>(())
        });
        match result {
            Ok(Ok(())) => self.submitted = true,
            Ok(Err(error)) => self.failure = Some(error),
            Err(_) => self.failure = Some("The Flow editor is no longer available."),
        }
        cx.notify();
    }
    fn blank(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitted {
            return;
        }
        let Some(target) = self.target else {
            return;
        };
        let result = self.flows.update(cx, |v, cx| {
            if !v.library.select(target) {
                return false;
            }
            window.close_dialog(cx);
            v.append_step(window, cx);
            true
        });
        self.submitted = result.unwrap_or(false);
    }
}
impl Render for TransferDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.error(cx);
        let new_flow = matches!(self.destination(cx), TransferDestination::New(_));
        div()
            .id("flow-transfer-dialog")
            .debug_selector(|| "flow-transfer-dialog".into())
            .flex()
            .flex_col()
            .gap_3()
            .text_size(m::BODY)
            .when(self.target.is_some(), |d| {
                d.child(
                    Button::new("flow-add-http")
                        .track_focus(&self.blank_focus)
                        .label("New HTTP request")
                        .icon(IconName::Terminal)
                        .w_full()
                        .large()
                        .on_click(cx.listener(|this, _, w, cx| this.blank(w, cx))),
                )
                .child(div().text_size(m::LABEL).child("From open HTTP requests"))
                .when(self.has_sources, |d| {
                    d.child(
                        Select::new(&self.source)
                            .id("flow-transfer-source")
                            .accessibility_label("Open HTTP request")
                            .placeholder("Choose a request")
                            .w_full(),
                    )
                })
                .when(!self.has_sources, |d| {
                    d.child(
                        div()
                            .text_color(MUTED.resolve(cx))
                            .child("No open HTTP requests. Start with a new request."),
                    )
                })
            })
            .when_some(self.snapshot.as_ref(), |d, s| {
                d.child(
                    div()
                        .p_3()
                        .rounded(m::RADIUS)
                        .bg(PANEL_ALT.resolve(cx))
                        .child(format!("{} · {}", s.method, s.name)),
                )
            })
            .when(self.target.is_none(), |d| {
                d.child(div().text_size(m::LABEL).child("Destination flow"))
                    .child(
                        Select::new(&self.destination)
                            .id("flow-transfer-destination")
                            .accessibility_label("Destination flow")
                            .w_full(),
                    )
                    .when(new_flow, |d| {
                        d.child(div().text_size(m::LABEL).child("Flow name")).child(
                            Input::new(&self.name)
                                .id("flow-transfer-name")
                                .aria_label("New flow name")
                                .w_full(),
                        )
                    })
            })
            .child(
                div()
                    .text_size(m::LABEL)
                    .text_color(MUTED.resolve(cx))
                    .child("Adds an independent copy. Your HTTP request stays unchanged."),
            )
            .when_some(error.filter(|_| self.snapshot.is_some()), |d, error| {
                d.child(
                    div()
                        .debug_selector(|| "flow-transfer-error".into())
                        .text_size(m::LABEL)
                        .text_color(ERROR.resolve(cx))
                        .child(error),
                )
            })
    }
}

#[cfg(test)]
mod tests;
