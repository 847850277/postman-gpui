const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Bounded, projection-aware history shared by the legacy adapters and `TextEditorState`.
///
/// User edits store the complete pre-edit state and invalidate Redo. Consecutive typing commits
/// may share one logical transaction until navigation or a discrete edit breaks the group.
#[derive(Clone, Debug)]
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
