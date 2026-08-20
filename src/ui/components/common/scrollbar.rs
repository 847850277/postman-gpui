/// Normalized geometry shared by the request-row and body-form scrollbars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarGeometry {
    pub thumb_top: f32,
    pub thumb_height: f32,
}

/// Builds a clamped scrollbar thumb from a visible/content ratio and GPUI's vertical offsets.
pub fn scrollbar_geometry(
    visible_fraction: f32,
    offset_y: f32,
    max_offset_y: f32,
) -> ScrollbarGeometry {
    let thumb_height = visible_fraction.clamp(0.18, 0.9);
    let progress = if max_offset_y > 0.0 {
        (-offset_y / max_offset_y).clamp(0.0, 1.0)
    } else {
        0.0
    };

    ScrollbarGeometry {
        thumb_top: progress * (1.0 - thumb_height),
        thumb_height,
    }
}

#[cfg(test)]
mod tests {
    use super::{scrollbar_geometry, ScrollbarGeometry};

    #[test]
    fn geometry_clamps_thumb_and_scroll_progress() {
        assert_eq!(
            scrollbar_geometry(0.5, -100.0, 200.0),
            ScrollbarGeometry {
                thumb_top: 0.25,
                thumb_height: 0.5,
            }
        );
        assert_eq!(scrollbar_geometry(0.01, 20.0, 100.0).thumb_height, 0.18);
        assert!((scrollbar_geometry(1.0, -200.0, 100.0).thumb_top - 0.1).abs() < f32::EPSILON);
    }
}
