use crate::ui::text_editor::TextRange;
use gpui::{fill, point, px, rgba, Bounds, Hsla, PaintQuad, Pixels, Point, ShapedLine};

const NEWLINE_SELECTION_WIDTH: Pixels = px(4.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) next_start: usize,
}

impl LineRange {
    pub(crate) fn len(self) -> usize {
        self.end - self.start
    }
}

/// Shaped multiline geometry shared by editable and read-only text surfaces. All ranges and hit
/// test results use canonical UTF-8 byte offsets, matching Rust slices and GPUI shaped lines.
#[derive(Clone)]
pub(crate) struct MultilineTextLayout {
    pub(crate) lines: Vec<ShapedLine>,
    pub(crate) ranges: Vec<LineRange>,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) line_height: Pixels,
}

impl MultilineTextLayout {
    pub(crate) fn new(
        lines: Vec<ShapedLine>,
        ranges: Vec<LineRange>,
        bounds: Bounds<Pixels>,
        line_height: Pixels,
    ) -> Self {
        Self {
            lines,
            ranges,
            bounds,
            line_height,
        }
    }

    pub(crate) fn matches(&self, text: &str) -> bool {
        let ranges = line_ranges(text);
        self.ranges == ranges
            && self.lines.len() == ranges.len()
            && (text.is_empty()
                || self
                    .lines
                    .iter()
                    .zip(ranges)
                    .all(|(line, range)| line.text.as_ref() == &text[range.start..range.end]))
    }

    pub(crate) fn hit_test_utf8(
        &self,
        text: &str,
        position: Point<Pixels>,
        fallback: usize,
    ) -> usize {
        if text.is_empty() {
            return 0;
        }
        if !self.matches(text) || self.lines.is_empty() {
            return floor_char_boundary(text, fallback.min(text.len()));
        }
        if position.y < self.bounds.top() {
            return 0;
        }
        if position.y > self.bounds.bottom() {
            return text.len();
        }

        let line_index = (((position.y - self.bounds.top()) / self.line_height).floor() as usize)
            .min(self.lines.len().saturating_sub(1));
        let range = self.ranges[line_index];
        let local = if position.x <= self.bounds.left() {
            0
        } else {
            floor_char_boundary(
                &text[range.start..range.end],
                self.lines[line_index]
                    .closest_index_for_x(position.x - self.bounds.left())
                    .min(range.len()),
            )
        };
        range.start + local
    }

    pub(crate) fn cursor_quad(&self, text: &str, cursor: usize, color: Hsla) -> Option<PaintQuad> {
        if !self.matches(text) || self.lines.is_empty() {
            return None;
        }
        let line_index = line_index_for_offset(&self.ranges, cursor);
        let range = self.ranges[line_index];
        let local = cursor.clamp(range.start, range.end) - range.start;
        Some(fill(
            Bounds::new(
                point(
                    self.bounds.left() + self.lines[line_index].x_for_index(local),
                    self.bounds.top() + self.line_height * line_index as f32,
                ),
                gpui::size(px(2.0), self.line_height),
            ),
            color,
        ))
    }

    pub(crate) fn selection_quads(&self, text: &str, selection: TextRange) -> Vec<PaintQuad> {
        if selection.is_empty() || !self.matches(text) || self.lines.is_empty() {
            return Vec::new();
        }
        let selection = selection.utf8();
        let start_line = line_index_for_offset(&self.ranges, selection.start);
        let end_line = line_index_for_offset(&self.ranges, selection.end);
        let mut quads = Vec::new();
        for line_index in start_line..=end_line {
            let range = self.ranges[line_index];
            let local_start = if line_index == start_line {
                selection.start.clamp(range.start, range.end) - range.start
            } else {
                0
            };
            let local_end = if line_index == end_line {
                selection.end.clamp(range.start, range.end) - range.start
            } else {
                range.len()
            };
            let start_x = self.lines[line_index].x_for_index(local_start);
            let mut end_x = self.lines[line_index].x_for_index(local_end);
            if selection.end > range.end && range.next_start > range.end {
                end_x += NEWLINE_SELECTION_WIDTH;
            }
            if end_x <= start_x {
                continue;
            }
            let top = self.bounds.top() + self.line_height * line_index as f32;
            quads.push(fill(
                Bounds::from_corners(
                    point(self.bounds.left() + start_x, top),
                    point(self.bounds.left() + end_x, top + self.line_height),
                ),
                rgba(0x3366_ff33),
            ));
        }
        quads
    }

    pub(crate) fn bounds_for_range(&self, text: &str, range: TextRange) -> Option<Bounds<Pixels>> {
        if !self.matches(text) || self.lines.is_empty() {
            return None;
        }
        let range = range.utf8();
        let start_line = line_index_for_offset(&self.ranges, range.start);
        let end_line = line_index_for_offset(&self.ranges, range.end);
        let start_range = self.ranges[start_line];
        let end_range = self.ranges[end_line];
        let start_local = range.start.clamp(start_range.start, start_range.end) - start_range.start;
        let end_local = range.end.clamp(end_range.start, end_range.end) - end_range.start;
        let top = self.bounds.top() + self.line_height * start_line as f32;
        let bottom = self.bounds.top() + self.line_height * (end_line + 1) as f32;
        if start_line == end_line {
            Some(Bounds::from_corners(
                point(
                    self.bounds.left() + self.lines[start_line].x_for_index(start_local),
                    top,
                ),
                point(
                    self.bounds.left() + self.lines[end_line].x_for_index(end_local),
                    bottom,
                ),
            ))
        } else {
            Some(Bounds::from_corners(
                point(self.bounds.left(), top),
                point(self.bounds.right(), bottom),
            ))
        }
    }
}

pub(crate) fn line_ranges(text: &str) -> Vec<LineRange> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                let next_start = if bytes.get(index + 1) == Some(&b'\n') {
                    index + 2
                } else {
                    index + 1
                };
                ranges.push(LineRange {
                    start,
                    end: index,
                    next_start,
                });
                start = next_start;
                index = next_start;
            }
            b'\n' => {
                let next_start = index + 1;
                ranges.push(LineRange {
                    start,
                    end: index,
                    next_start,
                });
                start = next_start;
                index = next_start;
            }
            _ => index += 1,
        }
    }
    ranges.push(LineRange {
        start,
        end: text.len(),
        next_start: text.len(),
    });
    ranges
}

pub(crate) fn line_index_for_offset(ranges: &[LineRange], offset: usize) -> usize {
    ranges
        .iter()
        .enumerate()
        .find_map(|(index, range)| (offset < range.next_start).then_some(index))
        .unwrap_or_else(|| ranges.len().saturating_sub(1))
}

pub(crate) fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_model_preserves_unicode_crlf_and_trailing_empty_lines() {
        assert_eq!(
            line_ranges("😀\r\n中\n"),
            vec![
                LineRange {
                    start: 0,
                    end: 4,
                    next_start: 6,
                },
                LineRange {
                    start: 6,
                    end: 9,
                    next_start: 10,
                },
                LineRange {
                    start: 10,
                    end: 10,
                    next_start: 10,
                },
            ]
        );
    }
}
