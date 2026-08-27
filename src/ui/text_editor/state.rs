use super::{
    history::EditHistory,
    offsets::{
        next_grapheme_boundary, next_word_boundary, previous_grapheme_boundary,
        previous_word_boundary, word_range_at, OffsetMap,
    },
    TextOffset, TextOffsetError, TextRange,
};
use std::{borrow::Cow, fmt, ops::Range};

/// Whether the editor accepts line separators as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextLineMode {
    SingleLine,
    Multiline,
}

/// Behavior shared by editable, masked, and read-only surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEditorPolicy {
    line_mode: TextLineMode,
    masked: bool,
    read_only: bool,
}

impl TextEditorPolicy {
    pub const fn single_line() -> Self {
        Self {
            line_mode: TextLineMode::SingleLine,
            masked: false,
            read_only: false,
        }
    }

    pub const fn multiline() -> Self {
        Self {
            line_mode: TextLineMode::Multiline,
            masked: false,
            read_only: false,
        }
    }

    pub const fn with_masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub const fn line_mode(self) -> TextLineMode {
        self.line_mode
    }

    pub const fn is_masked(self) -> bool {
        self.masked
    }

    pub const fn is_read_only(self) -> bool {
        self.read_only
    }

    fn normalize<'a>(self, text: &'a str) -> Cow<'a, str> {
        if self.line_mode == TextLineMode::SingleLine
            && text
                .chars()
                .any(|character| matches!(character, '\r' | '\n'))
        {
            Cow::Owned(
                text.chars()
                    .filter(|character| !matches!(character, '\r' | '\n'))
                    .collect(),
            )
        } else {
            Cow::Borrowed(text)
        }
    }
}

impl Default for TextEditorPolicy {
    fn default() -> Self {
        Self::single_line()
    }
}

/// Anchor and active cursor in canonical UTF-8 offsets. The range is always exposed normalized;
/// direction remains available through `is_reversed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    anchor: TextOffset,
    cursor: TextOffset,
}

impl TextSelection {
    pub const fn new(anchor: TextOffset, cursor: TextOffset) -> Self {
        Self { anchor, cursor }
    }

    pub const fn collapsed(offset: TextOffset) -> Self {
        Self {
            anchor: offset,
            cursor: offset,
        }
    }

    pub const fn anchor(self) -> TextOffset {
        self.anchor
    }

    pub const fn cursor(self) -> TextOffset {
        self.cursor
    }

    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.cursor)
    }

    pub const fn is_empty(self) -> bool {
        self.anchor.utf8() == self.cursor.utf8()
    }

    pub const fn is_reversed(self) -> bool {
        self.cursor.utf8() < self.anchor.utf8()
    }
}

/// Platform-facing selection adapter. Its offsets are exact UTF-16 code-unit boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utf16Selection {
    pub range: Range<usize>,
    pub reversed: bool,
}

/// Complete logical state restored by Undo/Redo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditorSnapshot {
    text: String,
    selection: TextSelection,
}

impl TextEditorSnapshot {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn selection(&self) -> TextSelection {
        self.selection
    }
}

/// How a completed edit participates in Undo grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTransaction {
    /// Clipboard operations, deletion, replacement, and other standalone edits.
    Discrete,
    /// Consecutive platform text commits coalesce until navigation or a discrete edit occurs.
    Typing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Changed,
    Unchanged,
}

/// Cursor movement shared by single-line and multiline adapters. Vertical movement remains a
/// layout concern and is intentionally resolved by a component before it supplies an offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMovement {
    PreviousGrapheme,
    NextGrapheme,
    PreviousWord,
    NextWord,
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionTransition {
    Started { range: TextRange },
    Updated { range: TextRange },
    Committed,
    Cancelled,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEditorError {
    Offset(TextOffsetError),
    ReadOnly,
    CompositionActive,
}

impl fmt::Display for TextEditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offset(error) => error.fmt(formatter),
            Self::ReadOnly => formatter.write_str("the text editor is read-only"),
            Self::CompositionActive => formatter.write_str(
                "an IME composition is active; commit or cancel it before this operation",
            ),
        }
    }
}

impl std::error::Error for TextEditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Offset(error) => Some(error),
            Self::ReadOnly | Self::CompositionActive => None,
        }
    }
}

impl From<TextOffsetError> for TextEditorError {
    fn from(error: TextOffsetError) -> Self {
        Self::Offset(error)
    }
}

#[derive(Clone, Debug)]
struct ActiveComposition {
    range: TextRange,
    before: TextEditorSnapshot,
}

/// Pure text editing, selection, Unicode-offset, IME, and history state.
///
/// The core has no GPUI, rendering, font, focus, or component ownership. Its canonical offsets are
/// exact UTF-8 byte boundaries; checked adapters cover platform UTF-16 code units, Unicode scalar
/// indices, and extended grapheme-cluster indices.
#[derive(Clone, Debug)]
pub struct TextEditorState {
    text: String,
    selection: TextSelection,
    composition: Option<ActiveComposition>,
    history: EditHistory<TextEditorSnapshot>,
    policy: TextEditorPolicy,
}

impl TextEditorState {
    pub fn new(text: impl Into<String>, policy: TextEditorPolicy) -> Self {
        let supplied = text.into();
        let text = policy.normalize(&supplied).into_owned();
        let end = TextOffset::from_valid_utf8(text.len());
        Self {
            text,
            selection: TextSelection::collapsed(end),
            composition: None,
            history: EditHistory::default(),
            policy,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn policy(&self) -> TextEditorPolicy {
        self.policy
    }

    /// Update only the disclosure policy; text, selection, composition, and history are unchanged.
    pub fn set_masked(&mut self, masked: bool) {
        self.policy.masked = masked;
    }

    pub const fn selection(&self) -> TextSelection {
        self.selection
    }

    pub fn selected_range(&self) -> TextRange {
        self.selection.range()
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selected_range().utf8()]
    }

    /// Masked editors never expose source text to clipboard adapters. Read-only editors may copy.
    pub fn selected_text_for_copy(&self) -> Option<&str> {
        (!self.policy.masked && !self.selection.is_empty()).then(|| self.selected_text())
    }

    pub fn snapshot(&self) -> TextEditorSnapshot {
        TextEditorSnapshot {
            text: self.text.clone(),
            selection: self.selection,
        }
    }

    pub fn composition_range(&self) -> Option<TextRange> {
        self.composition
            .as_ref()
            .map(|composition| composition.range)
    }

    /// Replace a projected model value without creating a user edit. History and composition are
    /// reset, while anchor and cursor are independently clamped to valid boundaries.
    pub fn project_text(&mut self, text: impl Into<String>) -> EditOutcome {
        let supplied = text.into();
        let projected = self.policy.normalize(&supplied).into_owned();
        let anchor = clamp_to_char_boundary(&projected, self.selection.anchor.utf8());
        let cursor = clamp_to_char_boundary(&projected, self.selection.cursor.utf8());
        let next_selection = TextSelection {
            anchor: TextOffset::from_valid_utf8(anchor),
            cursor: TextOffset::from_valid_utf8(cursor),
        };
        let changed = projected != self.text
            || next_selection != self.selection
            || self.composition.is_some();
        self.text = projected;
        self.selection = next_selection;
        self.composition = None;
        self.history.clear();
        if changed {
            EditOutcome::Changed
        } else {
            EditOutcome::Unchanged
        }
    }

    pub fn offset_from_utf8(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_utf8(offset)
    }

    pub fn offset_to_utf8(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        OffsetMap::new(&self.text).to_utf8(offset)
    }

    pub fn offset_from_utf16(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_utf16(offset)
    }

    pub fn offset_to_utf16(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        OffsetMap::new(&self.text).to_utf16(offset)
    }

    pub fn offset_from_scalar(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_scalar(offset)
    }

    pub fn offset_to_scalar(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        OffsetMap::new(&self.text).to_scalar(offset)
    }

    pub fn offset_from_grapheme(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_grapheme(offset)
    }

    pub fn offset_to_grapheme(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        OffsetMap::new(&self.text).to_grapheme(offset)
    }

    pub fn range_from_utf8(&self, range: Range<usize>) -> Result<TextRange, TextOffsetError> {
        OffsetMap::new(&self.text).range_from_utf8(range)
    }

    pub fn range_from_utf16(&self, range: Range<usize>) -> Result<TextRange, TextOffsetError> {
        OffsetMap::new(&self.text).range_from_utf16(range)
    }

    pub fn range_to_utf16(&self, range: TextRange) -> Result<Range<usize>, TextOffsetError> {
        OffsetMap::new(&self.text).range_to_utf16(range)
    }

    pub fn text_in_range(&self, range: TextRange) -> Result<&str, TextOffsetError> {
        let range = OffsetMap::new(&self.text).validate_range(range)?;
        Ok(&self.text[range.utf8()])
    }

    /// GPUI-style query returning both selected text and the normalized UTF-16 range actually used.
    pub fn text_for_utf16_range(
        &self,
        range: Range<usize>,
    ) -> Result<(&str, Range<usize>), TextOffsetError> {
        let range = self.range_from_utf16(range)?;
        let actual = self.range_to_utf16(range)?;
        Ok((&self.text[range.utf8()], actual))
    }

    pub fn selection_utf16(&self) -> Utf16Selection {
        let range = self
            .range_to_utf16(self.selected_range())
            .expect("TextEditorState selection must remain a valid text range");
        Utf16Selection {
            range,
            reversed: self.selection.is_reversed(),
        }
    }

    pub fn composition_range_utf16(&self) -> Option<Range<usize>> {
        self.composition_range().map(|range| {
            self.range_to_utf16(range)
                .expect("TextEditorState composition must remain a valid text range")
        })
    }

    pub fn set_selection(
        &mut self,
        anchor: TextOffset,
        cursor: TextOffset,
    ) -> Result<EditOutcome, TextEditorError> {
        self.ensure_idle()?;
        let map = OffsetMap::new(&self.text);
        let anchor = map.resolve_utf8(anchor.utf8())?;
        let cursor = map.resolve_utf8(cursor.utf8())?;
        Ok(self.set_selection_internal(TextSelection { anchor, cursor }))
    }

    pub fn set_selection_utf16(
        &mut self,
        range: Range<usize>,
        reversed: bool,
    ) -> Result<EditOutcome, TextEditorError> {
        self.ensure_idle()?;
        let range = self.range_from_utf16(range)?;
        let (anchor, cursor) = if reversed {
            (range.end(), range.start())
        } else {
            (range.start(), range.end())
        };
        Ok(self.set_selection_internal(TextSelection { anchor, cursor }))
    }

    pub fn select_all(&mut self) -> Result<EditOutcome, TextEditorError> {
        self.ensure_idle()?;
        Ok(self.set_selection_internal(TextSelection {
            anchor: TextOffset::ZERO,
            cursor: TextOffset::from_valid_utf8(self.text.len()),
        }))
    }

    pub fn select_word_at(&mut self, offset: TextOffset) -> Result<EditOutcome, TextEditorError> {
        self.ensure_idle()?;
        let offset = OffsetMap::new(&self.text).resolve_utf8(offset.utf8())?;
        let range = word_range_at(&self.text, offset.utf8());
        Ok(self.set_selection_internal(TextSelection {
            anchor: TextOffset::from_valid_utf8(range.start),
            cursor: TextOffset::from_valid_utf8(range.end),
        }))
    }

    pub fn move_cursor(
        &mut self,
        movement: TextMovement,
        extend_selection: bool,
    ) -> Result<EditOutcome, TextEditorError> {
        self.ensure_idle()?;
        let range = self.selected_range();
        let cursor = self.selection.cursor.utf8();
        let next = if !extend_selection && !range.is_empty() {
            match movement {
                TextMovement::PreviousGrapheme | TextMovement::PreviousWord => range.start().utf8(),
                TextMovement::NextGrapheme | TextMovement::NextWord => range.end().utf8(),
                TextMovement::DocumentStart => 0,
                TextMovement::DocumentEnd => self.text.len(),
            }
        } else {
            match movement {
                TextMovement::PreviousGrapheme => previous_grapheme_boundary(&self.text, cursor),
                TextMovement::NextGrapheme => next_grapheme_boundary(&self.text, cursor),
                TextMovement::PreviousWord => previous_word_boundary(&self.text, cursor),
                TextMovement::NextWord => next_word_boundary(&self.text, cursor),
                TextMovement::DocumentStart => 0,
                TextMovement::DocumentEnd => self.text.len(),
            }
        };
        let next = TextOffset::from_valid_utf8(next);
        let selection = if extend_selection {
            TextSelection {
                anchor: self.selection.anchor,
                cursor: next,
            }
        } else {
            TextSelection::collapsed(next)
        };
        Ok(self.set_selection_internal(selection))
    }

    pub fn insert_text(
        &mut self,
        text: &str,
        transaction: EditTransaction,
    ) -> Result<EditOutcome, TextEditorError> {
        self.replace_range(self.selected_range(), text, transaction)
    }

    pub fn replace_range(
        &mut self,
        range: TextRange,
        replacement: &str,
        transaction: EditTransaction,
    ) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let range = OffsetMap::new(&self.text).validate_range(range)?;
        let replacement = self.policy.normalize(replacement);
        Ok(self.apply_replacement(range, &replacement, transaction))
    }

    /// Adapter for `EntityInputHandler::replace_text_in_range`; `None` means current selection.
    pub fn replace_utf16_range(
        &mut self,
        range: Option<Range<usize>>,
        replacement: &str,
        transaction: EditTransaction,
    ) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let range = match range {
            Some(range) => self.range_from_utf16(range)?,
            None => self.selected_range(),
        };
        self.replace_range(range, replacement, transaction)
    }

    pub fn delete_backward(&mut self) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let range = if self.selection.is_empty() {
            let cursor = self.selection.cursor.utf8();
            TextRange::new(
                TextOffset::from_valid_utf8(previous_grapheme_boundary(&self.text, cursor)),
                self.selection.cursor,
            )
        } else {
            self.selected_range()
        };
        if range.is_empty() {
            self.history.break_typing_group();
            return Ok(EditOutcome::Unchanged);
        }
        self.replace_range(range, "", EditTransaction::Discrete)
    }

    pub fn delete_forward(&mut self) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let range = if self.selection.is_empty() {
            let cursor = self.selection.cursor.utf8();
            TextRange::new(
                self.selection.cursor,
                TextOffset::from_valid_utf8(next_grapheme_boundary(&self.text, cursor)),
            )
        } else {
            self.selected_range()
        };
        if range.is_empty() {
            self.history.break_typing_group();
            return Ok(EditOutcome::Unchanged);
        }
        self.replace_range(range, "", EditTransaction::Discrete)
    }

    pub fn undo(&mut self) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let current = self.snapshot();
        let Some(previous) = self.history.undo(current) else {
            return Ok(EditOutcome::Unchanged);
        };
        self.restore_snapshot(previous);
        Ok(EditOutcome::Changed)
    }

    pub fn redo(&mut self) -> Result<EditOutcome, TextEditorError> {
        self.ensure_editable_and_idle()?;
        let current = self.snapshot();
        let Some(next) = self.history.redo(current) else {
            return Ok(EditOutcome::Unchanged);
        };
        self.restore_snapshot(next);
        Ok(EditOutcome::Changed)
    }

    pub fn break_edit_group(&mut self) {
        self.history.break_typing_group();
    }

    /// Clear Undo/Redo at a model-projection boundary without changing visible editor state.
    pub fn clear_edit_history(&mut self) {
        self.history.clear();
    }

    /// Start or update an IME composition using absolute UTF-16 replacement offsets and a
    /// selection range relative to the newly marked text. All ranges are validated before state
    /// changes, so a rejected platform range leaves the editor untouched.
    pub fn update_composition_utf16(
        &mut self,
        range: Option<Range<usize>>,
        marked_text: &str,
        selected_range_in_marked_text: Option<Range<usize>>,
    ) -> Result<CompositionTransition, TextEditorError> {
        self.ensure_editable()?;
        let target = match range {
            Some(range) => self.range_from_utf16(range)?,
            None => self
                .composition
                .as_ref()
                .map(|composition| composition.range)
                .unwrap_or_else(|| self.selected_range()),
        };
        let target = OffsetMap::new(&self.text).validate_range(target)?;
        let marked_text = self.policy.normalize(marked_text).into_owned();
        let relative_selection = match selected_range_in_marked_text {
            Some(range) => OffsetMap::new(&marked_text).range_from_utf16(range)?,
            None => TextRange::collapsed(TextOffset::from_valid_utf8(marked_text.len())),
        };

        let updating = self.composition.is_some();
        let before = self
            .composition
            .as_ref()
            .map(|composition| composition.before.clone())
            .unwrap_or_else(|| self.snapshot());
        let mut replacement =
            String::with_capacity(self.text.len() - target.utf8().len() + marked_text.len());
        replacement.push_str(&self.text[..target.start().utf8()]);
        replacement.push_str(&marked_text);
        replacement.push_str(&self.text[target.end().utf8()..]);

        let marked_start = target.start().utf8();
        let marked_end = marked_start + marked_text.len();
        let marked_range = TextRange::new(
            TextOffset::from_valid_utf8(marked_start),
            TextOffset::from_valid_utf8(marked_end),
        );
        self.text = replacement;
        self.selection = TextSelection {
            anchor: TextOffset::from_valid_utf8(marked_start + relative_selection.start().utf8()),
            cursor: TextOffset::from_valid_utf8(marked_start + relative_selection.end().utf8()),
        };
        self.composition = Some(ActiveComposition {
            range: marked_range,
            before,
        });
        self.history.break_typing_group();

        Ok(if updating {
            CompositionTransition::Updated {
                range: marked_range,
            }
        } else {
            CompositionTransition::Started {
                range: marked_range,
            }
        })
    }

    pub fn commit_composition(&mut self) -> CompositionTransition {
        let Some(composition) = self.composition.take() else {
            return CompositionTransition::Idle;
        };
        if self.snapshot() != composition.before {
            self.history.record(composition.before);
        }
        CompositionTransition::Committed
    }

    pub fn cancel_composition(&mut self) -> CompositionTransition {
        let Some(composition) = self.composition.take() else {
            return CompositionTransition::Idle;
        };
        self.restore_snapshot(composition.before);
        self.history.break_typing_group();
        CompositionTransition::Cancelled
    }

    fn apply_replacement(
        &mut self,
        range: TextRange,
        replacement_text: &str,
        transaction: EditTransaction,
    ) -> EditOutcome {
        let before = self.snapshot();
        let mut replacement =
            String::with_capacity(self.text.len() - range.utf8().len() + replacement_text.len());
        replacement.push_str(&self.text[..range.start().utf8()]);
        replacement.push_str(replacement_text);
        replacement.push_str(&self.text[range.end().utf8()..]);
        let cursor = TextOffset::from_valid_utf8(range.start().utf8() + replacement_text.len());
        let next_selection = TextSelection::collapsed(cursor);
        if replacement == self.text && next_selection == self.selection {
            return EditOutcome::Unchanged;
        }
        match transaction {
            EditTransaction::Discrete => self.history.record(before),
            EditTransaction::Typing => self.history.record_typing(before),
        }
        self.text = replacement;
        self.selection = next_selection;
        EditOutcome::Changed
    }

    fn set_selection_internal(&mut self, selection: TextSelection) -> EditOutcome {
        self.history.break_typing_group();
        if self.selection == selection {
            EditOutcome::Unchanged
        } else {
            self.selection = selection;
            EditOutcome::Changed
        }
    }

    fn restore_snapshot(&mut self, snapshot: TextEditorSnapshot) {
        self.text = snapshot.text;
        self.selection = snapshot.selection;
        self.composition = None;
    }

    fn ensure_editable(&self) -> Result<(), TextEditorError> {
        if self.policy.read_only {
            Err(TextEditorError::ReadOnly)
        } else {
            Ok(())
        }
    }

    fn ensure_idle(&self) -> Result<(), TextEditorError> {
        if self.composition.is_some() {
            Err(TextEditorError::CompositionActive)
        } else {
            Ok(())
        }
    }

    fn ensure_editable_and_idle(&self) -> Result<(), TextEditorError> {
        self.ensure_editable()?;
        self.ensure_idle()
    }
}

impl Default for TextEditorState {
    fn default() -> Self {
        Self::new(String::new(), TextEditorPolicy::default())
    }
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
