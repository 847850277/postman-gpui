//! Retained per-Flow readonly source view. No runtime values enter this projection.
use crate::ui::theme::{metrics as m, ACCENT, CODE_STRING, FONT_MONO, LINE, MUTED, PANEL, WARNING};
use gpui::{
    div, relative, AppContext, ClipboardItem, Context, Entity, HighlightStyle, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, TextDecoration, TextDecorationCollection},
    Disableable, Sizable, Theme,
};
use postman_flow::{write_flow_yaml, FlowDocument};

pub(super) struct YamlPreview {
    pub(super) editor: Entity<EditorState>,
    revision: Option<u64>,
    keys: Vec<std::ops::Range<usize>>,
    decorations: TextDecorationCollection,
    _theme: Subscription,
    pub(super) source: Option<String>,
    pub(super) status: String,
    pub(super) current: bool,
}
impl YamlPreview {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .language("yaml")
                .context_menu(false)
                .searchable(false)
                .folding(false)
                .line_number(true)
                .scroll_beyond_last_line(Some(0));
            editor.set_readonly(true, cx);
            editor.set_soft_wrap(true, window, cx);
            editor
        });
        let decorations = editor.update(cx, |editor, cx| {
            editor.create_decorations_collection(Vec::new(), cx)
        });
        let theme = cx.observe_global::<Theme>(|this, cx| this.refresh_syntax(cx));
        Self {
            keys: Vec::new(),
            decorations,
            _theme: theme,
            editor,
            revision: None,
            source: None,
            status: String::new(),
            current: false,
        }
    }
    pub(super) fn refresh(
        &mut self,
        document: &FlowDocument,
        revision: u64,
        pending: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.current = false;
        if pending {
            self.status = if self.source.is_some() {
                "Last applied document · Stale — fix unapplied edits and check their locations in Logs. Copy is disabled."
            } else { "YAML unavailable — fix unapplied document edits. Use Check to locate them." }.into();
        } else {
            match write_flow_yaml(document) {
                Ok(source) => {
                    // Preserve selection, undo state and scroll when serialization is unchanged.
                    if self.source.as_ref() != Some(&source) {
                        self.editor.update(cx, |editor, cx| {
                            editor.set_value(source.clone(), window, cx)
                        });
                        self.keys = yaml_keys(&source);
                        self.source = Some(source);
                        self.refresh_syntax(cx);
                    }
                    self.current = true;
                    self.status = "Readonly YAML · Current document".into();
                }
                Err(error) => {
                    self.status = format!(
                        "YAML unavailable · {:?} · {error}. Copy is disabled.",
                        error.code
                    );
                }
            }
        }
        cx.notify();
    }
}
impl YamlPreview {
    fn refresh_syntax(&self, cx: &mut Context<Self>) {
        self.decorations.set(
            self.keys
                .iter()
                .map(|range| {
                    TextDecoration::new(
                        range.clone(),
                        HighlightStyle {
                            color: Some(ACCENT.resolve(cx).into()),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            cx,
        );
    }
}
/// Accent plain generated keys as in the HTML reference; Kit owns text shaping and selection.
fn yaml_keys(source: &str) -> Vec<std::ops::Range<usize>> {
    let mut offset = 0;
    source
        .split_inclusive('\n')
        .filter_map(|line| {
            let start = offset;
            offset += line.len();
            let field = line
                .trim_start()
                .strip_prefix("- ")
                .unwrap_or(line.trim_start());
            let length = field
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
                .count();
            (length > 0 && field.as_bytes().get(length) == Some(&b':')).then(|| {
                let start = start + line.len() - field.len();
                start..start + length
            })
        })
        .collect()
}
impl Render for YamlPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("flow-yaml")
            .debug_selector(|| "flow-yaml".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .id("flow-yaml-status")
                            .role(gpui::Role::Status)
                            .flex_1()
                            .min_w_0()
                            .text_size(m::COMPACT_LABEL)
                            .text_color(if self.current { MUTED } else { WARNING }.resolve(cx))
                            .child(self.status.clone()),
                    )
                    .child(
                        Button::new("flow-yaml-copy")
                            .small()
                            .ghost()
                            .label("Copy YAML")
                            .disabled(!self.current)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.current {
                                    if let Some(source) = &this.source {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            source.clone(),
                                        ));
                                    }
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "flow-yaml-editor".into())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        Editor::new(&self.editor)
                            .readonly(true)
                            .appearance(false)
                            .bordered(false)
                            .h(relative(1.))
                            .min_h_0()
                            .font_family(FONT_MONO)
                            .text_size(m::CODE)
                            .text_color(CODE_STRING.resolve(cx))
                            .line_height(relative(m::CODE_LINE_HEIGHT))
                            .rounded_none()
                            .aria_label("Readonly Flow YAML"),
                    ),
            )
    }
}
