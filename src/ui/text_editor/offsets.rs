use std::{fmt, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

/// The coordinate system named by an offset conversion error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOffsetUnit {
    Utf8,
    Utf16,
    Scalar,
    Grapheme,
}

impl fmt::Display for TextOffsetUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Utf8 => "UTF-8 byte",
            Self::Utf16 => "UTF-16 code unit",
            Self::Scalar => "Unicode scalar",
            Self::Grapheme => "grapheme",
        })
    }
}

/// A checked offset could not be represented at an exact text boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextOffsetError {
    OutOfBounds {
        unit: TextOffsetUnit,
        offset: usize,
        maximum: usize,
    },
    NotBoundary {
        unit: TextOffsetUnit,
        offset: usize,
    },
}

impl fmt::Display for TextOffsetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds {
                unit,
                offset,
                maximum,
            } => write!(
                formatter,
                "{unit} offset {offset} exceeds the maximum {maximum}"
            ),
            Self::NotBoundary { unit, offset } => {
                write!(formatter, "{unit} offset {offset} is not an exact boundary")
            }
        }
    }
}

impl std::error::Error for TextOffsetError {}

/// Canonical editor offset: an exact UTF-8 byte boundary in the current text.
///
/// UTF-8 bytes are used internally because Rust string slicing and GPUI shaped-line indices use
/// this coordinate system. Values received from another state are revalidated by every public
/// mutation before they can index the current text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextOffset(usize);

impl TextOffset {
    pub const ZERO: Self = Self(0);

    pub const fn utf8(self) -> usize {
        self.0
    }

    pub(super) const fn from_valid_utf8(offset: usize) -> Self {
        Self(offset)
    }
}

/// A normalized, half-open range of canonical UTF-8 text offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRange {
    start: TextOffset,
    end: TextOffset,
}

impl TextRange {
    pub fn new(first: TextOffset, second: TextOffset) -> Self {
        if first <= second {
            Self {
                start: first,
                end: second,
            }
        } else {
            Self {
                start: second,
                end: first,
            }
        }
    }

    pub const fn collapsed(offset: TextOffset) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub const fn start(self) -> TextOffset {
        self.start
    }

    pub const fn end(self) -> TextOffset {
        self.end
    }

    pub const fn is_empty(self) -> bool {
        self.start.0 == self.end.0
    }

    pub fn utf8(self) -> Range<usize> {
        self.start.0..self.end.0
    }
}

pub(super) struct OffsetMap<'a> {
    text: &'a str,
}

impl<'a> OffsetMap<'a> {
    pub(super) const fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub(super) fn resolve_utf8(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        if offset > self.text.len() {
            return Err(TextOffsetError::OutOfBounds {
                unit: TextOffsetUnit::Utf8,
                offset,
                maximum: self.text.len(),
            });
        }
        if !self.text.is_char_boundary(offset) {
            return Err(TextOffsetError::NotBoundary {
                unit: TextOffsetUnit::Utf8,
                offset,
            });
        }
        Ok(TextOffset(offset))
    }

    pub(super) fn to_utf8(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        self.resolve_utf8(offset.0).map(TextOffset::utf8)
    }

    pub(super) fn resolve_utf16(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        let maximum = self.text.encode_utf16().count();
        if offset > maximum {
            return Err(TextOffsetError::OutOfBounds {
                unit: TextOffsetUnit::Utf16,
                offset,
                maximum,
            });
        }

        let mut utf16 = 0;
        for (utf8, character) in self.text.char_indices() {
            if utf16 == offset {
                return Ok(TextOffset(utf8));
            }
            let next = utf16 + character.len_utf16();
            if offset < next {
                return Err(TextOffsetError::NotBoundary {
                    unit: TextOffsetUnit::Utf16,
                    offset,
                });
            }
            utf16 = next;
        }
        Ok(TextOffset(self.text.len()))
    }

    pub(super) fn to_utf16(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        let utf8 = self.to_utf8(offset)?;
        Ok(self.text[..utf8].encode_utf16().count())
    }

    pub(super) fn resolve_scalar(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        let maximum = self.text.chars().count();
        if offset > maximum {
            return Err(TextOffsetError::OutOfBounds {
                unit: TextOffsetUnit::Scalar,
                offset,
                maximum,
            });
        }
        if offset == maximum {
            return Ok(TextOffset(self.text.len()));
        }
        Ok(TextOffset(
            self.text
                .char_indices()
                .nth(offset)
                .map(|(utf8, _)| utf8)
                .unwrap_or(self.text.len()),
        ))
    }

    pub(super) fn to_scalar(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        let utf8 = self.to_utf8(offset)?;
        Ok(self.text[..utf8].chars().count())
    }

    pub(super) fn resolve_grapheme(&self, offset: usize) -> Result<TextOffset, TextOffsetError> {
        let maximum = self.text.graphemes(true).count();
        if offset > maximum {
            return Err(TextOffsetError::OutOfBounds {
                unit: TextOffsetUnit::Grapheme,
                offset,
                maximum,
            });
        }
        if offset == maximum {
            return Ok(TextOffset(self.text.len()));
        }
        Ok(TextOffset(
            self.text
                .grapheme_indices(true)
                .nth(offset)
                .map(|(utf8, _)| utf8)
                .unwrap_or(self.text.len()),
        ))
    }

    pub(super) fn to_grapheme(&self, offset: TextOffset) -> Result<usize, TextOffsetError> {
        let utf8 = self.to_utf8(offset)?;
        if utf8 == self.text.len() {
            return Ok(self.text.graphemes(true).count());
        }
        self.text
            .grapheme_indices(true)
            .position(|(boundary, _)| boundary == utf8)
            .ok_or(TextOffsetError::NotBoundary {
                unit: TextOffsetUnit::Grapheme,
                offset: utf8,
            })
    }

    pub(super) fn range_from_utf8(
        &self,
        range: Range<usize>,
    ) -> Result<TextRange, TextOffsetError> {
        Ok(TextRange::new(
            self.resolve_utf8(range.start)?,
            self.resolve_utf8(range.end)?,
        ))
    }

    pub(super) fn range_from_utf16(
        &self,
        range: Range<usize>,
    ) -> Result<TextRange, TextOffsetError> {
        Ok(TextRange::new(
            self.resolve_utf16(range.start)?,
            self.resolve_utf16(range.end)?,
        ))
    }

    pub(super) fn range_to_utf16(&self, range: TextRange) -> Result<Range<usize>, TextOffsetError> {
        Ok(self.to_utf16(range.start())?..self.to_utf16(range.end())?)
    }

    pub(super) fn validate_range(&self, range: TextRange) -> Result<TextRange, TextOffsetError> {
        Ok(TextRange::new(
            self.resolve_utf8(range.start().utf8())?,
            self.resolve_utf8(range.end().utf8())?,
        ))
    }
}

pub(crate) fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    text.grapheme_indices(true)
        .rev()
        .find_map(|(boundary, _)| (boundary < offset).then_some(boundary))
        .unwrap_or(0)
}

pub(crate) fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    text.grapheme_indices(true)
        .find_map(|(boundary, _)| (boundary > offset).then_some(boundary))
        .unwrap_or(text.len())
}

pub(crate) fn previous_word_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset.min(text.len()));
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
    let offset = floor_char_boundary(text, offset.min(text.len()));
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
            return *index;
        }
    }
    text.len()
}

pub(crate) fn word_range_at(text: &str, offset: usize) -> Range<usize> {
    let graphemes = text.grapheme_indices(true).collect::<Vec<_>>();
    if graphemes.is_empty() {
        return 0..0;
    }
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let target = graphemes
        .iter()
        .position(|(start, grapheme)| offset < *start + grapheme.len())
        .unwrap_or(graphemes.len() - 1);
    let target_is_word = is_word_grapheme(graphemes[target].1);

    let mut first = target;
    while first > 0 && is_word_grapheme(graphemes[first - 1].1) == target_is_word {
        first -= 1;
    }
    let mut last = target + 1;
    while last < graphemes.len() && is_word_grapheme(graphemes[last].1) == target_is_word {
        last += 1;
    }
    let end = graphemes
        .get(last)
        .map(|(start, _)| *start)
        .unwrap_or(text.len());
    graphemes[first].0..end
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn is_word_grapheme(grapheme: &str) -> bool {
    grapheme
        .chars()
        .any(|character| character.is_alphanumeric() || character == '_')
}
