pub(crate) use crate::ui::text_editor::{next_word_boundary, previous_word_boundary, EditHistory};

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
