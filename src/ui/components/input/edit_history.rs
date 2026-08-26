use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_HISTORY_LIMIT: usize = 100;

/// One text-editor state used by URL, single-line, and multiline request inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEditSnapshot {
    pub(crate) text: String,
    pub(crate) selection: Range<usize>,
    pub(crate) selection_reversed: bool,
}

/// Bounded, projection-aware edit history shared by custom GPUI inputs.
///
/// ViewModel projection clears this history so Undo can never cross request-tab or History replay
/// boundaries. User edits record the complete pre-edit state and invalidate Redo.
#[derive(Debug)]
pub(crate) struct EditHistory<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
    coalescing_typing: bool,
}

impl<T> Default for EditHistory<T> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: DEFAULT_HISTORY_LIMIT,
            coalescing_typing: false,
        }
    }
}

impl<T: Clone + PartialEq> EditHistory<T> {
    pub(crate) fn record(&mut self, state: T) {
        self.coalescing_typing = false;
        self.push_undo(state);
    }

    /// Coalesces consecutive platform text commits into one native-feeling typing transaction.
    /// Cursor movement, clipboard commands, structural edits, Undo/Redo, and projection call
    /// `break_typing_group` (directly or through `record`) before the next commit.
    pub(crate) fn record_typing(&mut self, state: T) {
        if !self.coalescing_typing {
            self.push_undo(state);
            self.coalescing_typing = true;
        }
    }

    fn push_undo(&mut self, state: T) {
        if self.undo.last() == Some(&state) {
            return;
        }
        self.undo.push(state);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn undo(&mut self, current: T) -> Option<T> {
        self.coalescing_typing = false;
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    pub(crate) fn redo(&mut self, current: T) -> Option<T> {
        self.coalescing_typing = false;
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.coalescing_typing = false;
    }

    pub(crate) fn break_typing_group(&mut self) {
        self.coalescing_typing = false;
    }
}

pub(crate) fn previous_word_boundary(text: &str, offset: usize) -> usize {
    let graphemes = text
        .grapheme_indices(true)
        .take_while(|(index, _)| *index < offset)
        .collect::<Vec<_>>();
    if graphemes.is_empty() {
        return 0;
    }

    let mut position = graphemes.len();
    while position > 0 && !is_word_grapheme(graphemes[position - 1].1) {
        position -= 1;
    }
    while position > 0 && is_word_grapheme(graphemes[position - 1].1) {
        position -= 1;
    }
    graphemes
        .get(position)
        .map(|(index, _)| *index)
        .unwrap_or(0)
}

pub(crate) fn next_word_boundary(text: &str, offset: usize) -> usize {
    let graphemes = text
        .grapheme_indices(true)
        .filter(|(index, _)| *index >= offset)
        .collect::<Vec<_>>();
    let Some((_, first)) = graphemes.first() else {
        return text.len();
    };
    let starts_in_word = is_word_grapheme(first);
    for (index, grapheme) in graphemes.iter().skip(1) {
        if starts_in_word != is_word_grapheme(grapheme) {
            if starts_in_word {
                return *index;
            }
            if is_word_grapheme(grapheme) {
                return *index;
            }
        }
    }
    text.len()
}

fn is_word_grapheme(grapheme: &str) -> bool {
    grapheme
        .chars()
        .any(|character| character.is_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::{next_word_boundary, previous_word_boundary, EditHistory};

    #[test]
    fn history_is_bounded_projection_aware_and_invalidates_redo() {
        let mut history = EditHistory::default();
        history.record("a".to_string());
        history.record("ab".to_string());
        assert_eq!(history.undo("abc".to_string()).as_deref(), Some("ab"));
        assert_eq!(history.redo("ab".to_string()).as_deref(), Some("abc"));
        history.record("replacement".to_string());
        assert!(history.redo("new".to_string()).is_none());
        history.clear();
        assert!(history.undo("new".to_string()).is_none());
    }

    #[test]
    fn consecutive_typing_is_one_undo_transaction() {
        let mut history = EditHistory::default();
        history.record_typing(String::new());
        history.record_typing("a".to_string());
        history.record_typing("ab".to_string());
        assert_eq!(history.undo("abc".to_string()).as_deref(), Some(""));

        history.record_typing("replacement".to_string());
        history.break_typing_group();
        history.record_typing("replacement-a".to_string());
        assert_eq!(
            history.undo("replacement-ab".to_string()).as_deref(),
            Some("replacement-a")
        );
    }

    #[test]
    fn word_boundaries_keep_unicode_graphemes_and_separators_intact() {
        let text = "alpha 中文 value_2";
        assert_eq!(previous_word_boundary(text, text.len()), 13);
        assert_eq!(previous_word_boundary(text, 12), 6);
        assert_eq!(next_word_boundary(text, 0), 5);
        assert_eq!(next_word_boundary(text, 5), 6);
        assert_eq!(next_word_boundary(text, 6), 12);
    }
}
