use super::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
struct OffsetCase {
    name: &'static str,
    text: &'static str,
    scalar_boundaries: &'static [(usize, usize, usize)],
    grapheme_boundaries: &'static [usize],
}

const OFFSET_CASES: &[OffsetCase] = &[
    OffsetCase {
        name: "empty",
        text: "",
        scalar_boundaries: &[(0, 0, 0)],
        grapheme_boundaries: &[0],
    },
    OffsetCase {
        name: "ascii",
        text: "abc",
        scalar_boundaries: &[(0, 0, 0), (1, 1, 1), (2, 2, 2), (3, 3, 3)],
        grapheme_boundaries: &[0, 1, 2, 3],
    },
    OffsetCase {
        name: "cjk",
        text: "中文",
        scalar_boundaries: &[(0, 0, 0), (3, 1, 1), (6, 2, 2)],
        grapheme_boundaries: &[0, 3, 6],
    },
    OffsetCase {
        name: "surrogate-pair emoji",
        text: "A😀B",
        scalar_boundaries: &[(0, 0, 0), (1, 1, 1), (5, 3, 2), (6, 4, 3)],
        grapheme_boundaries: &[0, 1, 5, 6],
    },
    OffsetCase {
        name: "combining sequence",
        text: "e\u{301}x",
        scalar_boundaries: &[(0, 0, 0), (1, 1, 1), (3, 2, 2), (4, 3, 3)],
        grapheme_boundaries: &[0, 3, 4],
    },
    OffsetCase {
        name: "emoji ZWJ sequence",
        text: "👩‍💻",
        scalar_boundaries: &[(0, 0, 0), (4, 2, 1), (7, 3, 2), (11, 5, 3)],
        grapheme_boundaries: &[0, 11],
    },
    OffsetCase {
        name: "CRLF",
        text: "a\r\nb",
        scalar_boundaries: &[(0, 0, 0), (1, 1, 1), (2, 2, 2), (3, 3, 3), (4, 4, 4)],
        grapheme_boundaries: &[0, 1, 3, 4],
    },
];

#[test]
fn checked_offset_tables_cover_ascii_cjk_emoji_combining_zwj_crlf_and_empty_text() {
    for case in OFFSET_CASES {
        let state = TextEditorState::new(case.text, TextEditorPolicy::multiline());
        for &(utf8, utf16, scalar) in case.scalar_boundaries {
            let from_utf8 = state
                .offset_from_utf8(utf8)
                .unwrap_or_else(|error| panic!("{} UTF-8 {utf8}: {error}", case.name));
            assert_eq!(
                state.offset_from_utf16(utf16),
                Ok(from_utf8),
                "{} UTF-16 {utf16}",
                case.name
            );
            assert_eq!(
                state.offset_to_utf16(from_utf8),
                Ok(utf16),
                "{} UTF-8 -> UTF-16",
                case.name
            );
            assert_eq!(
                state.offset_from_scalar(scalar),
                Ok(from_utf8),
                "{} scalar {scalar}",
                case.name
            );
            assert_eq!(
                state.offset_to_scalar(from_utf8),
                Ok(scalar),
                "{} UTF-8 -> scalar",
                case.name
            );
        }
        for (grapheme, &utf8) in case.grapheme_boundaries.iter().enumerate() {
            let boundary = state.offset_from_utf8(utf8).unwrap();
            assert_eq!(
                state.offset_from_grapheme(grapheme),
                Ok(boundary),
                "{} grapheme {grapheme}",
                case.name
            );
            assert_eq!(
                state.offset_to_grapheme(boundary),
                Ok(grapheme),
                "{} UTF-8 -> grapheme",
                case.name
            );
        }
    }
}

#[test]
fn checked_conversions_reject_partial_code_units_scalars_and_graphemes() {
    let cjk = TextEditorState::new("中", TextEditorPolicy::multiline());
    assert!(matches!(
        cjk.offset_from_utf8(1),
        Err(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Utf8,
            offset: 1,
        })
    ));
    assert!(matches!(
        cjk.offset_from_utf8(4),
        Err(TextOffsetError::OutOfBounds {
            unit: TextOffsetUnit::Utf8,
            offset: 4,
            maximum: 3,
        })
    ));

    let emoji = TextEditorState::new("A😀B", TextEditorPolicy::multiline());
    assert!(matches!(
        emoji.offset_from_utf16(2),
        Err(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Utf16,
            offset: 2,
        })
    ));

    let combining = TextEditorState::new("e\u{301}", TextEditorPolicy::multiline());
    let scalar_boundary_inside_grapheme = combining.offset_from_scalar(1).unwrap();
    assert!(matches!(
        combining.offset_to_grapheme(scalar_boundary_inside_grapheme),
        Err(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Grapheme,
            offset: 1,
        })
    ));
    assert!(matches!(
        combining.offset_from_scalar(3),
        Err(TextOffsetError::OutOfBounds {
            unit: TextOffsetUnit::Scalar,
            offset: 3,
            maximum: 2,
        })
    ));
}

#[test]
fn utf16_ranges_normalize_and_report_the_actual_checked_range() {
    let state = TextEditorState::new("A😀B", TextEditorPolicy::multiline());
    let reversed_range = std::ops::Range { start: 3, end: 1 };
    let normalized = state.range_from_utf16(reversed_range.clone()).unwrap();
    assert_eq!(normalized.utf8(), 1..5);
    assert_eq!(state.range_to_utf16(normalized).unwrap(), 1..3);
    assert_eq!(
        state.text_for_utf16_range(reversed_range).unwrap(),
        ("😀", 1..3)
    );
}

#[test]
fn conversion_round_trips_hold_for_a_combinatorial_unicode_corpus() {
    let atoms = ["", "a", "中", "😀", "e\u{301}", "👩‍💻", "\r", "\n"];
    for left in atoms {
        for right in atoms {
            let text = format!("{left}{right}");
            let state = TextEditorState::new(&text, TextEditorPolicy::multiline());
            for utf8 in char_boundaries(&text) {
                let offset = state.offset_from_utf8(utf8).unwrap();
                let utf16 = state.offset_to_utf16(offset).unwrap();
                assert_eq!(state.offset_from_utf16(utf16), Ok(offset), "{text:?}");
                let scalar = state.offset_to_scalar(offset).unwrap();
                assert_eq!(state.offset_from_scalar(scalar), Ok(offset), "{text:?}");
            }
            for (grapheme, utf8) in grapheme_boundaries(&text).into_iter().enumerate() {
                let offset = state.offset_from_grapheme(grapheme).unwrap();
                assert_eq!(offset.utf8(), utf8, "{text:?}");
                assert_eq!(state.offset_to_grapheme(offset), Ok(grapheme), "{text:?}");
            }
        }
    }
}

#[test]
fn every_selection_pair_is_normalized_without_losing_anchor_direction() {
    for case in OFFSET_CASES {
        let boundaries = char_boundaries(case.text);
        for &anchor_utf8 in &boundaries {
            for &cursor_utf8 in &boundaries {
                let mut state = TextEditorState::new(case.text, TextEditorPolicy::multiline());
                let anchor = state.offset_from_utf8(anchor_utf8).unwrap();
                let cursor = state.offset_from_utf8(cursor_utf8).unwrap();
                state.set_selection(anchor, cursor).unwrap();
                let selection = state.selection();
                assert_eq!(selection.anchor(), anchor, "{} anchor", case.name);
                assert_eq!(selection.cursor(), cursor, "{} cursor", case.name);
                assert_eq!(
                    selection.range().utf8(),
                    anchor_utf8.min(cursor_utf8)..anchor_utf8.max(cursor_utf8),
                    "{} normalized range",
                    case.name
                );
                assert_eq!(selection.is_reversed(), cursor_utf8 < anchor_utf8);
                assert_invariants(&state);
            }
        }
    }
}

#[test]
fn foreign_offsets_are_revalidated_and_failed_mutations_are_atomic() {
    let ascii = TextEditorState::new("ab", TextEditorPolicy::single_line());
    let byte_one = ascii.offset_from_utf8(1).unwrap();
    let byte_two = ascii.offset_from_utf8(2).unwrap();
    let mut cjk = TextEditorState::new("中", TextEditorPolicy::single_line());
    let before = cjk.snapshot();

    assert!(matches!(
        cjk.set_selection(byte_one, byte_one),
        Err(TextEditorError::Offset(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Utf8,
            offset: 1,
        }))
    ));
    assert_eq!(cjk.snapshot(), before);
    assert!(matches!(
        cjk.replace_range(
            TextRange::new(byte_two, byte_two),
            "x",
            EditTransaction::Discrete,
        ),
        Err(TextEditorError::Offset(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Utf8,
            offset: 2,
        }))
    ));
    assert_eq!(cjk.snapshot(), before);
}

#[test]
fn replace_undo_redo_round_trips_text_cursor_and_selection_for_every_range() {
    for case in OFFSET_CASES {
        let boundaries = char_boundaries(case.text);
        for &first in &boundaries {
            for &second in &boundaries {
                let mut state = TextEditorState::new(case.text, TextEditorPolicy::multiline());
                let anchor = state.offset_from_utf8(second).unwrap();
                let cursor = state.offset_from_utf8(first).unwrap();
                state.set_selection(anchor, cursor).unwrap();
                let before = state.snapshot();
                state
                    .replace_range(state.selected_range(), "新😀", EditTransaction::Discrete)
                    .unwrap();
                let after = state.snapshot();
                assert_ne!(after, before, "{} {first}..{second}", case.name);
                assert_eq!(state.undo().unwrap(), EditOutcome::Changed);
                assert_eq!(state.snapshot(), before, "{} Undo", case.name);
                assert_eq!(state.redo().unwrap(), EditOutcome::Changed);
                assert_eq!(state.snapshot(), after, "{} Redo", case.name);
                assert_invariants(&state);
            }
        }
    }
}

#[test]
fn typing_is_one_transaction_and_navigation_starts_the_next_group() {
    let mut state = TextEditorState::default();
    state.insert_text("a", EditTransaction::Typing).unwrap();
    state.insert_text("中", EditTransaction::Typing).unwrap();
    state.insert_text("😀", EditTransaction::Typing).unwrap();
    let typed = state.snapshot();
    assert_eq!(state.text(), "a中😀");
    state.undo().unwrap();
    assert_eq!(state.snapshot(), TextEditorState::default().snapshot());
    state.redo().unwrap();
    assert_eq!(state.snapshot(), typed);

    state
        .move_cursor(TextMovement::PreviousGrapheme, false)
        .unwrap();
    let before_next_group = state.snapshot();
    state.insert_text("x", EditTransaction::Typing).unwrap();
    state.insert_text("y", EditTransaction::Typing).unwrap();
    state.undo().unwrap();
    assert_eq!(state.snapshot(), before_next_group);
}

#[test]
fn grapheme_and_word_navigation_and_deletion_share_one_boundary_contract() {
    let mut state = TextEditorState::new("e\u{301}👩‍💻x", TextEditorPolicy::multiline());
    let start = state.offset_from_utf8(0).unwrap();
    state.set_selection(start, start).unwrap();
    state
        .move_cursor(TextMovement::NextGrapheme, false)
        .unwrap();
    assert_eq!(state.selection().cursor().utf8(), 3);
    state
        .move_cursor(TextMovement::NextGrapheme, false)
        .unwrap();
    assert_eq!(state.selection().cursor().utf8(), 14);
    state.delete_backward().unwrap();
    assert_eq!(state.text(), "e\u{301}x");
    assert_eq!(state.selection().cursor().utf8(), 3);
    state.undo().unwrap();
    assert_eq!(state.text(), "e\u{301}👩‍💻x");

    let mut words = TextEditorState::new("alpha 中文 value_2", TextEditorPolicy::single_line());
    let start = words.offset_from_utf8(0).unwrap();
    words.set_selection(start, start).unwrap();
    words.move_cursor(TextMovement::NextWord, false).unwrap();
    assert_eq!(words.selection().cursor().utf8(), 5);
    words.move_cursor(TextMovement::NextWord, false).unwrap();
    assert_eq!(words.selection().cursor().utf8(), 6);
    words.move_cursor(TextMovement::NextWord, true).unwrap();
    assert_eq!(words.selected_range().utf8(), 6..12);
}

#[test]
fn ime_ranges_round_trip_for_bmp_surrogates_combining_cjk_and_zwj_text() {
    let marked_texts = ["abc", "中文", "😀", "e\u{301}", "👩‍💻", "\r\n", ""];
    for marked in marked_texts {
        let mut state = TextEditorState::new("前😀后", TextEditorPolicy::multiline());
        state.set_selection_utf16(1..3, false).unwrap();
        let before = state.snapshot();
        let marked_utf16_len = marked.encode_utf16().count();
        let transition = state
            .update_composition_utf16(None, marked, Some(0..marked_utf16_len))
            .unwrap();
        let expected_range = 1..1 + marked_utf16_len;
        assert!(matches!(transition, CompositionTransition::Started { .. }));
        assert_eq!(
            state.composition_range_utf16(),
            Some(expected_range.clone())
        );
        assert_eq!(state.selection_utf16().range, expected_range);
        assert_invariants(&state);
        assert_eq!(state.cancel_composition(), CompositionTransition::Cancelled);
        assert_eq!(state.snapshot(), before, "marked text {marked:?}");
    }
}

#[test]
fn ime_relative_selection_is_resolved_inside_marked_text_after_a_surrogate_prefix() {
    let mut state = TextEditorState::new("😀tail", TextEditorPolicy::multiline());
    state.set_selection_utf16(2..2, false).unwrap();
    state
        .update_composition_utf16(None, "e\u{301}中", Some(1..3))
        .unwrap();

    assert_eq!(state.text(), "😀e\u{301}中tail");
    assert_eq!(state.composition_range_utf16(), Some(2..5));
    assert_eq!(state.selection_utf16().range, 3..5);
    assert_invariants(&state);
}

#[test]
fn composition_start_update_commit_cancel_and_history_are_explicit() {
    let mut state = TextEditorState::new("A😀B", TextEditorPolicy::multiline());
    state.set_selection_utf16(1..3, false).unwrap();
    let before = state.snapshot();

    assert!(matches!(
        state
            .update_composition_utf16(None, "中", Some(1..1))
            .unwrap(),
        CompositionTransition::Started { .. }
    ));
    assert_eq!(state.text(), "A中B");
    assert_eq!(state.composition_range_utf16(), Some(1..2));
    assert_eq!(
        state.insert_text("x", EditTransaction::Typing),
        Err(TextEditorError::CompositionActive)
    );

    assert!(matches!(
        state
            .update_composition_utf16(None, "中文", Some(1..2))
            .unwrap(),
        CompositionTransition::Updated { .. }
    ));
    assert_eq!(state.text(), "A中文B");
    assert_eq!(state.composition_range_utf16(), Some(1..3));
    assert_eq!(state.selection_utf16().range, 2..3);
    let committed = state.snapshot();
    assert_eq!(state.commit_composition(), CompositionTransition::Committed);
    assert_eq!(state.composition_range(), None);
    assert_eq!(state.undo().unwrap(), EditOutcome::Changed);
    assert_eq!(state.snapshot(), before);
    assert_eq!(state.redo().unwrap(), EditOutcome::Changed);
    assert_eq!(state.snapshot(), committed);

    state.set_selection_utf16(1..3, true).unwrap();
    let before_cancel = state.snapshot();
    state.update_composition_utf16(None, "取消", None).unwrap();
    assert_eq!(state.cancel_composition(), CompositionTransition::Cancelled);
    assert_eq!(state.snapshot(), before_cancel);
    assert_eq!(state.cancel_composition(), CompositionTransition::Idle);
}

#[test]
fn rejected_ime_ranges_leave_text_selection_and_composition_unchanged() {
    let mut state = TextEditorState::new("A😀B", TextEditorPolicy::multiline());
    let before = state.snapshot();
    assert!(matches!(
        state.update_composition_utf16(Some(2..3), "x", None),
        Err(TextEditorError::Offset(TextOffsetError::NotBoundary {
            unit: TextOffsetUnit::Utf16,
            offset: 2,
        }))
    ));
    assert_eq!(state.snapshot(), before);
    assert_eq!(state.composition_range(), None);
}

#[test]
fn policies_cover_single_line_multiline_masked_and_read_only_without_forks() {
    let mut single = TextEditorState::new("a\r\nb", TextEditorPolicy::single_line());
    assert_eq!(single.text(), "ab");
    single
        .insert_text("\r中\n", EditTransaction::Discrete)
        .unwrap();
    assert_eq!(single.text(), "ab中");

    let multiline = TextEditorState::new("a\r\nb", TextEditorPolicy::multiline());
    assert_eq!(multiline.text(), "a\r\nb");

    let mut masked = TextEditorState::new(
        "secret😀",
        TextEditorPolicy::single_line().with_masked(true),
    );
    masked.select_all().unwrap();
    assert_eq!(masked.selected_text(), "secret😀");
    assert_eq!(masked.selected_text_for_copy(), None);

    let mut read_only = TextEditorState::new(
        "response中文",
        TextEditorPolicy::multiline().with_read_only(true),
    );
    read_only.select_all().unwrap();
    assert_eq!(read_only.selected_text_for_copy(), Some("response中文"));
    assert_eq!(
        read_only.insert_text("blocked", EditTransaction::Discrete),
        Err(TextEditorError::ReadOnly)
    );
    assert_eq!(read_only.delete_backward(), Err(TextEditorError::ReadOnly));
    assert_eq!(read_only.undo(), Err(TextEditorError::ReadOnly));
    assert_eq!(read_only.text(), "response中文");
    assert_invariants(&read_only);
}

#[test]
fn projection_clamps_both_selection_ends_and_clears_history_and_composition() {
    let mut state = TextEditorState::new("A😀B", TextEditorPolicy::multiline());
    state
        .set_selection_utf16(std::ops::Range { start: 4, end: 1 }, true)
        .unwrap();
    state
        .insert_text("typed", EditTransaction::Discrete)
        .unwrap();
    state.update_composition_utf16(None, "组合", None).unwrap();
    assert_eq!(state.project_text("中x"), EditOutcome::Changed);
    assert_eq!(state.composition_range(), None);
    assert_invariants(&state);
    assert_eq!(state.undo().unwrap(), EditOutcome::Unchanged);
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn grapheme_boundaries(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn assert_invariants(state: &TextEditorState) {
    let selection = state.selection();
    assert!(state.text().is_char_boundary(selection.anchor().utf8()));
    assert!(state.text().is_char_boundary(selection.cursor().utf8()));
    assert!(selection.range().start() <= selection.range().end());
    let utf16 = state.selection_utf16();
    let round_trip = state.range_from_utf16(utf16.range).unwrap();
    assert_eq!(round_trip, selection.range());
    if let Some(composition) = state.composition_range() {
        assert!(state.text().is_char_boundary(composition.start().utf8()));
        assert!(state.text().is_char_boundary(composition.end().utf8()));
        let utf16 = state.range_to_utf16(composition).unwrap();
        assert_eq!(state.range_from_utf16(utf16).unwrap(), composition);
    }
}
