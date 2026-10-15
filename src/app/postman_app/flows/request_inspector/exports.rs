//! Export drafts have stable row identities independent of names, paths, or list positions.
use super::view::{field, hint};
use super::*;
use crate::ui::theme::LINE;
use gpui::{
    div, prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::Input,
        Disableable, Sizable,
    },
};
use postman_flow::{validate_definition_name, validate_json_path, ResponseExport};
use std::collections::HashMap;

pub(super) struct ExportRow {
    pub(super) id: u64,
    anchor: gpui::ScrollAnchor,
    name: Entity<InputState>,
    path: Entity<InputState>,
    sensitive: bool,
    // A typed projection of this row's last applied draft, never a runtime value.
    value: Option<ResponseExport>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl RequestInspector {
    pub(super) fn export_row(
        id: u64,
        value: Option<ResponseExport>,
        scroll: &ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ExportRow {
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Name");
            input.set_value(
                value.as_ref().map(|v| v.name.clone()).unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        let path = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("$.json.id");
            input.set_value(
                value
                    .as_ref()
                    .map(|v| v.json_path.clone())
                    .unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        let subscriptions = [&name, &path]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, move |this, _, event, cx| {
                    if this.readonly.is_none()
                        && matches!(event, InputEvent::Change)
                        && this.exports.iter().any(|r| r.id == id)
                    {
                        this.apply_exports(cx);
                    }
                })
            })
            .collect();
        ExportRow {
            id,
            anchor: gpui::ScrollAnchor::for_handle(scroll.clone()),
            name,
            path,
            sensitive: value.as_ref().is_some_and(|v| v.sensitive),
            value,
            error: None,
            _subscriptions: subscriptions,
        }
    }
    fn apply_exports(&mut self, cx: &mut Context<Self>) {
        let candidates: Vec<_> = self
            .exports
            .iter()
            .map(|row| {
                let name = row.name.read(cx).value().to_string();
                let json_path = row.path.read(cx).value().to_string();
                validate_definition_name(&name).map_err(str::to_owned)?;
                validate_json_path(&json_path)?;
                Ok(ResponseExport {
                    name,
                    json_path,
                    sensitive: row.sensitive,
                })
            })
            .collect();
        let previous: Vec<_> = self.exports.iter().map(|r| r.value.clone()).collect();
        let resolved = reconcile_exports(candidates, &previous);
        for (row, result) in self.exports.iter_mut().zip(resolved) {
            row.error = result.as_ref().err().cloned();
            if let Ok(value) = result {
                row.value = Some(value);
            }
        }
        cx.emit(StepEdit::Exports(
            self.exports
                .iter()
                .filter_map(|r| r.value.clone())
                .collect(),
        ));
        cx.notify();
    }
    fn add_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly.is_some() {
            return;
        }
        let id = self.allocate_row();
        let row = Self::export_row(id, None, &self.tab_scroll[2], window, cx);
        row.name.focus_handle(cx).focus(window, cx);
        row.anchor.scroll_to(window, cx);
        self.exports.push(row);
        cx.emit(StepEdit::Invalid);
        cx.notify();
    }
    fn remove_export(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly.is_some() {
            return;
        }
        let Some(ix) = self.exports.iter().position(|r| r.id == id) else {
            return;
        };
        self.exports.remove(ix);
        self.exports
            .get(ix)
            .or_else(|| self.exports.last())
            .map(|r| r.name.focus_handle(cx))
            .unwrap_or_else(|| self.add_export_focus.clone())
            .focus(window, cx);
        self.apply_exports(cx);
    }
    pub(super) fn exports_content(&self, cx: &mut Context<Self>) -> AnyElement {
        div().flex().flex_col().gap_4().min_w_0()
            .child(hint("Extract values from the JSON response for later steps.", false, cx))
            .when(self.exports.is_empty(), |d| d.child(hint("No exports. Add a name and a JSONPath to declare a value.", false, cx)))
            .children(self.exports.iter().map(|row| {
                let id = row.id;
                div().id(("flow-export-row", id)).anchor_scroll(Some(row.anchor.clone())).flex().flex_col().gap_2().min_w_0().pb_3().border_b_1().border_color(LINE.resolve(cx))
                    .child(div().flex().items_start().gap_2().min_w_0()
                        .child(div().flex_1().min_w_0().child(field("Name", Input::new(&row.name).id(("flow-export-name", id)).aria_label("Export name").readonly(self.readonly.is_some()).w_full(), cx)))
                        .child(div().flex_1().min_w_0().child(field("JSONPath", Input::new(&row.path).id(("flow-export-path", id)).aria_label("Export JSONPath").readonly(self.readonly.is_some()).w_full(), cx)))
                        .when(self.readonly.is_none(), |d| d.child(div().pt_5().child(Button::new(("flow-remove-export", id)).icon(IconName::Trash).ghost().small().accessibility_label("Remove export")
                            .on_click(cx.listener(move |this, _, w, cx| this.remove_export(id, w, cx)))))))
                    .child(Checkbox::new(("flow-export-sensitive", id)).label("Sensitive").checked(row.sensitive).disabled(self.readonly.is_some()).small()
                        .on_change(cx.listener(move |this, checked, _, cx| {
                            if this.readonly.is_none() {
                                if let Some(row) = this.exports.iter_mut().find(|r| r.id == id) { row.sensitive = *checked; this.apply_exports(cx); }
                            }
                        })))
                    .when_some(row.error.clone(), |d, e| d.child(hint(format!("{e} Draft retained; last valid export kept."), true, cx)))
            }))
            .when(self.readonly.is_none(), |d| d.child(div().flex().items_start().child(Button::new("flow-add-export").label("Add export").icon(IconName::Plus).ghost().small().track_focus(&self.add_export_focus)
                .on_click(cx.listener(|this, _, w, cx| this.add_export(w, cx))))))
            .child(hint("Renaming or removing an export leaves existing references unchanged. Sensitive marks values for runtime redaction.", false, cx))
            .into_any_element()
    }
}

/// Resolve candidate definitions together. An invalid row reserves its last valid name so a
/// different edit cannot accidentally create duplicates in the applied document. Revisit collisions
/// after each fallback: falling back to an earlier name can expose another conflict.
fn reconcile_exports(
    mut candidates: Vec<Result<ResponseExport, String>>,
    previous: &[Option<ResponseExport>],
) -> Vec<Result<ResponseExport, String>> {
    loop {
        let mut names: HashMap<&str, Vec<usize>> = HashMap::new();
        for (ix, result) in candidates.iter().enumerate() {
            if let Some(value) = result.as_ref().ok().or(previous[ix].as_ref()) {
                names.entry(&value.name).or_default().push(ix);
            }
        }
        let mut invalid = Vec::new();
        for (name, rows) in names.into_iter().filter(|(_, rows)| rows.len() > 1) {
            // An unchanged committed name owns its identity; reject new claimants without
            // blocking edits to the established row's path or sensitivity.
            let owners: Vec<_> = rows
                .iter()
                .copied()
                .filter(|ix| previous[*ix].as_ref().is_some_and(|v| v.name == name))
                .collect();
            let owner = (owners.len() == 1).then(|| owners[0]);
            for ix in rows {
                if Some(ix) != owner && candidates[ix].is_ok() {
                    invalid.push((ix, format!("Duplicate export name '{name}'.")));
                }
            }
        }
        if invalid.is_empty() {
            break;
        }
        for (ix, error) in invalid {
            candidates[ix] = Err(error);
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_resolution_preserves_owners_and_rechecks_fallback_chains() {
        let value = |name| ResponseExport::json(name, "$");
        let previous = vec![Some(value("a")), Some(value("b")), Some(value("c"))];
        let mut b = value("b");
        b.json_path = "$.changed".into();
        let result = reconcile_exports(
            vec![Ok(value("b")), Ok(b.clone()), Ok(value("a"))],
            &previous,
        );
        assert!(result[0].is_err());
        assert_eq!(result[1], Ok(b));
        assert!(result[2].is_err());
        let result = reconcile_exports(
            vec![Ok(value("b")), Ok(value("a")), Ok(value("c"))],
            &previous,
        );
        assert!(
            result.iter().all(Result::is_ok),
            "coherent renames can swap names"
        );
        let result = reconcile_exports(vec![Ok(value("new")), Ok(value("new"))], &[None, None]);
        assert!(
            result.iter().all(Result::is_err),
            "no arbitrary winner for new duplicates"
        );
    }
}
