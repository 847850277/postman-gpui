//! UI-independent text editing and Unicode range contracts.
//!
//! Rendering, font shaping, hit-testing, focus, and GPUI entity ownership stay in component
//! adapters. This module owns text, selection, navigation, IME composition, and edit history.

mod history;
mod offsets;
mod read_only;
mod state;

pub use offsets::{TextOffset, TextOffsetError, TextOffsetUnit, TextRange};
pub use read_only::ReadOnlyTextSelection;
pub use state::{
    CompositionTransition, EditOutcome, EditTransaction, TextEditorError, TextEditorPolicy,
    TextEditorSnapshot, TextEditorState, TextLineMode, TextMovement, TextSelection, Utf16Selection,
};

#[cfg(test)]
pub(crate) use history::EditHistory;
#[cfg(test)]
pub(crate) use offsets::{next_word_boundary, previous_word_boundary};

#[cfg(test)]
mod tests;
