use super::{
    offsets::{word_range_at, OffsetMap},
    TextOffset, TextOffsetError, TextRange, TextSelection,
};

/// Read-only text projection plus a normalized directional selection.
///
/// The adapter intentionally exposes no replacement, deletion, paste, composition, or history
/// APIs. Response views can project display text and select/copy slices, but cannot mutate the
/// response payload that produced that projection.
#[derive(Clone, Debug)]
pub struct ReadOnlyTextSelection {
    text: String,
    selection: TextSelection,
    dragging: bool,
}

impl Default for ReadOnlyTextSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadOnlyTextSelection {
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            selection: TextSelection::collapsed(TextOffset::ZERO),
            dragging: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn selection(&self) -> TextSelection {
        self.selection
    }

    pub fn selected_range(&self) -> TextRange {
        self.selection.range()
    }

    pub const fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selected_range().utf8()]
    }

    pub fn selected_text_for_copy(&self) -> Option<&str> {
        (!self.selection.is_empty()).then(|| self.selected_text())
    }

    /// Project a new visible representation and clamp both directional endpoints independently.
    /// Selection survives layout-only changes and remains valid when formatted/raw text changes.
    pub fn project_text(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if self.text == text {
            return false;
        }
        let anchor = clamp_to_char_boundary(&text, self.selection.anchor().utf8());
        let cursor = clamp_to_char_boundary(&text, self.selection.cursor().utf8());
        self.text = text;
        self.selection = TextSelection::new(
            TextOffset::from_valid_utf8(anchor),
            TextOffset::from_valid_utf8(cursor),
        );
        self.dragging = false;
        true
    }

    pub fn offset_from_utf8(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_utf8(offset)
    }

    pub fn offset_from_scalar(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        OffsetMap::new(&self.text).resolve_scalar(offset)
    }

    pub fn collapse_to(&mut self, offset: TextOffset) -> Result<bool, TextOffsetError> {
        let offset = OffsetMap::new(&self.text).resolve_utf8(offset.utf8())?;
        let next = TextSelection::collapsed(offset);
        let changed = self.selection != next;
        self.selection = next;
        Ok(changed)
    }

    pub fn extend_to(&mut self, offset: TextOffset) -> Result<bool, TextOffsetError> {
        let offset = OffsetMap::new(&self.text).resolve_utf8(offset.utf8())?;
        let next = TextSelection::new(self.selection.anchor(), offset);
        let changed = self.selection != next;
        self.selection = next;
        Ok(changed)
    }

    pub fn select_word_at(&mut self, offset: TextOffset) -> Result<bool, TextOffsetError> {
        let offset = OffsetMap::new(&self.text).resolve_utf8(offset.utf8())?;
        let range = word_range_at(&self.text, offset.utf8());
        let map = OffsetMap::new(&self.text);
        let next = TextSelection::new(map.resolve_utf8(range.start)?, map.resolve_utf8(range.end)?);
        let changed = self.selection != next;
        self.selection = next;
        self.dragging = false;
        Ok(changed)
    }

    pub fn select_all(&mut self) -> bool {
        let next = TextSelection::new(
            TextOffset::ZERO,
            TextOffset::from_valid_utf8(self.text.len()),
        );
        let changed = self.selection != next;
        self.selection = next;
        self.dragging = false;
        changed
    }

    pub fn clear_selection(&mut self) -> bool {
        self.dragging = false;
        let next = TextSelection::collapsed(self.selection.cursor());
        let changed = self.selection != next;
        self.selection = next;
        changed
    }

    pub fn reset_selection(&mut self) -> bool {
        self.dragging = false;
        let next = TextSelection::collapsed(TextOffset::ZERO);
        let changed = self.selection != next;
        self.selection = next;
        changed
    }

    pub fn pointer_down(
        &mut self,
        offset: TextOffset,
        extend: bool,
        click_count: usize,
    ) -> Result<bool, TextOffsetError> {
        self.dragging = click_count < 2;
        if click_count >= 2 {
            self.select_word_at(offset)
        } else if extend {
            self.extend_to(offset)
        } else {
            self.collapse_to(offset)
        }
    }

    pub fn pointer_move(&mut self, offset: TextOffset) -> Result<bool, TextOffsetError> {
        if !self.dragging {
            return Ok(false);
        }
        self.extend_to(offset)
    }

    pub fn pointer_up(&mut self) {
        self.dragging = false;
    }
}

fn clamp_to_char_boundary(text: &str, requested: usize) -> usize {
    let mut requested = requested.min(text.len());
    while !text.is_char_boundary(requested) {
        requested -= 1;
    }
    requested
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_selection_copy_uses_exact_utf8_boundaries_and_direction() {
        let mut selection = ReadOnlyTextSelection::new();
        selection.project_text("A😀中e\u{301} Z");
        let anchor = selection.offset_from_scalar(4).unwrap();
        let cursor = selection.offset_from_scalar(1).unwrap();
        selection.collapse_to(anchor).unwrap();
        selection.extend_to(cursor).unwrap();

        assert!(selection.selection().is_reversed());
        assert_eq!(selection.selected_text_for_copy(), Some("😀中e"));
        assert_eq!(selection.selected_range().utf8(), 1..9);
    }

    #[test]
    fn word_selection_projection_and_clear_remain_normalized() {
        let mut selection = ReadOnlyTextSelection::new();
        selection.project_text("alpha 世界 😀 omega");
        let world = selection.offset_from_scalar(7).unwrap();
        selection.pointer_down(world, false, 2).unwrap();
        assert_eq!(selection.selected_text(), "世界");

        selection.project_text("短");
        assert!(selection.selected_range().end().utf8() <= selection.text().len());
        selection.clear_selection();
        assert!(selection.selected_range().is_empty());
        assert_eq!(selection.text(), "短");
    }

    #[test]
    fn drag_selection_never_exposes_a_text_mutation_surface() {
        let mut selection = ReadOnlyTextSelection::new();
        selection.project_text("first\nsecond");
        let start = selection.offset_from_utf8(0).unwrap();
        let end = selection.offset_from_utf8(selection.text().len()).unwrap();
        selection.pointer_down(start, false, 1).unwrap();
        selection.pointer_move(end).unwrap();
        selection.pointer_up();

        assert_eq!(selection.selected_text(), "first\nsecond");
        assert_eq!(selection.text(), "first\nsecond");
        assert!(!selection.is_dragging());
    }
}
