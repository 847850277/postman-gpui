use crate::{
    app::{KeyValueRow, RequestPane},
    ui::components::common::scrollbar::scrollbar_geometry,
};

pub(super) use crate::ui::components::common::scrollbar::ScrollbarGeometry as RowScrollbarGeometry;

const REQUEST_PANEL_BASE_HEIGHT: f32 = 360.0;
const PARAM_ROWS_AT_BASE_HEIGHT: usize = 2;
const PARAM_ROW_PITCH: f32 = 46.0;
const PARAM_PANEL_MAX_VISIBLE_ROWS: usize = 6;
const HEADER_PANEL_MAX_VISIBLE_ROWS: usize = 4;
const URL_ENCODED_PANEL_MAX_VISIBLE_ROWS: usize = 6;
const REQUEST_EDITOR_RESERVED_HEIGHT: f32 = 400.0;

pub(super) const REQUEST_HEAD_HEIGHT: f32 = 46.0;
pub(super) const REQUEST_COMPOSER_GAP: f32 = 12.0;
pub(super) const WORKSPACE_CONTENT_PADDING: f32 = 12.0;
pub(super) const RESPONSE_RESIZE_TRACK_HEIGHT: f32 = 12.0;
pub(super) const RESPONSE_PANEL_MIN_HEIGHT: f32 = 180.0;
pub(super) const REQUEST_PANEL_RESIZE_MIN_HEIGHT: f32 = 300.0;

/// UI-only split state shared by the composer and its row panes. `None` keeps the existing
/// row-driven automatic height; dragging the Response divider installs a manual height.
#[derive(Default)]
pub(super) struct RequestPanelLayout {
    manual_height: Option<f32>,
}

impl RequestPanelLayout {
    pub(super) fn resolved_height(
        &self,
        pane: RequestPane,
        visible_rows: usize,
        viewport_height: f32,
    ) -> f32 {
        self.manual_height
            .unwrap_or_else(|| adaptive_request_panel_height(pane, visible_rows, viewport_height))
    }

    pub(super) fn set_manual_height(&mut self, height: f32) -> bool {
        if self.manual_height == Some(height) {
            return false;
        }
        self.manual_height = Some(height);
        true
    }

    pub(super) fn reset(&mut self) -> bool {
        self.manual_height.take().is_some()
    }
}

pub(super) fn resizable_request_panel_height_bounds(workspace_content_height: f32) -> (f32, f32) {
    let reserved_height = WORKSPACE_CONTENT_PADDING * 2.0
        + REQUEST_HEAD_HEIGHT
        + REQUEST_COMPOSER_GAP
        + RESPONSE_RESIZE_TRACK_HEIGHT
        + RESPONSE_PANEL_MIN_HEIGHT;
    let maximum = (workspace_content_height - reserved_height).max(REQUEST_PANEL_RESIZE_MIN_HEIGHT);
    (REQUEST_PANEL_RESIZE_MIN_HEIGHT, maximum)
}

pub(super) fn header_row_complete(row: &KeyValueRow) -> bool {
    !row.key.trim().is_empty() && !row.value.trim().is_empty()
}

pub(super) fn adaptive_request_panel_height(
    pane: RequestPane,
    visible_param_rows: usize,
    viewport_height: f32,
) -> f32 {
    if !matches!(
        pane,
        RequestPane::Params | RequestPane::Headers | RequestPane::Body
    ) {
        return REQUEST_PANEL_BASE_HEIGHT;
    }

    let max_visible_rows = match pane {
        RequestPane::Params => PARAM_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Headers => HEADER_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Body => URL_ENCODED_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Authorization
        | RequestPane::Scripts
        | RequestPane::Tests
        | RequestPane::Options => {
            unreachable!("non-row panes returned above")
        }
    };
    let expandable_rows = max_visible_rows - PARAM_ROWS_AT_BASE_HEIGHT;
    let added_rows = visible_param_rows
        .saturating_sub(PARAM_ROWS_AT_BASE_HEIGHT)
        .min(expandable_rows);
    let desired_height = REQUEST_PANEL_BASE_HEIGHT + PARAM_ROW_PITCH * added_rows as f32;
    let maximum_height = REQUEST_PANEL_BASE_HEIGHT + PARAM_ROW_PITCH * expandable_rows as f32;
    let viewport_height = (viewport_height - REQUEST_EDITOR_RESERVED_HEIGHT)
        .clamp(REQUEST_PANEL_BASE_HEIGHT, maximum_height);

    desired_height.min(viewport_height)
}

pub(super) fn visible_row_capacity(pane: RequestPane, panel_height: f32) -> usize {
    let max_visible_rows = match pane {
        RequestPane::Params => PARAM_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Headers => HEADER_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Body => URL_ENCODED_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Authorization
        | RequestPane::Scripts
        | RequestPane::Tests
        | RequestPane::Options => return 0,
    };
    let row_delta = ((panel_height - REQUEST_PANEL_BASE_HEIGHT) / PARAM_ROW_PITCH).floor() as isize;
    (PARAM_ROWS_AT_BASE_HEIGHT as isize + row_delta).clamp(1, max_visible_rows as isize) as usize
}

pub(super) fn row_scrollbar_geometry(
    visible_rows: usize,
    visible_capacity: usize,
    offset_y: f32,
    max_offset_y: f32,
) -> Option<RowScrollbarGeometry> {
    if visible_rows <= visible_capacity || visible_capacity == 0 {
        return None;
    }

    Some(scrollbar_geometry(
        visible_capacity as f32 / visible_rows as f32,
        offset_y,
        max_offset_y,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        adaptive_request_panel_height, resizable_request_panel_height_bounds,
        row_scrollbar_geometry, visible_row_capacity, RequestPanelLayout, RowScrollbarGeometry,
    };
    use crate::app::RequestPane;

    #[test]
    fn params_panel_grows_by_row_then_caps_for_scrolling() {
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 1, 980.0),
            360.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 2, 980.0),
            360.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 3, 980.0),
            406.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 6, 980.0),
            544.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 30, 980.0),
            544.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 6, 820.0),
            420.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Headers, 30, 980.0),
            452.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Body, 5, 980.0),
            498.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Body, 20, 980.0),
            544.0
        );

        assert_eq!(visible_row_capacity(RequestPane::Params, 360.0), 2);
        assert_eq!(visible_row_capacity(RequestPane::Params, 300.0), 1);
        assert_eq!(visible_row_capacity(RequestPane::Params, 406.0), 3);
        assert_eq!(visible_row_capacity(RequestPane::Params, 544.0), 6);
        assert_eq!(visible_row_capacity(RequestPane::Headers, 452.0), 4);
        assert_eq!(visible_row_capacity(RequestPane::Body, 544.0), 6);
        assert_eq!(row_scrollbar_geometry(6, 6, 0.0, 0.0), None);
        assert_eq!(
            row_scrollbar_geometry(12, 6, -100.0, 200.0),
            Some(RowScrollbarGeometry {
                thumb_top: 0.25,
                thumb_height: 0.5,
            })
        );

        let mut layout = RequestPanelLayout::default();
        assert_eq!(layout.resolved_height(RequestPane::Params, 6, 980.0), 544.0);
        assert!(layout.set_manual_height(320.0));
        assert_eq!(layout.resolved_height(RequestPane::Params, 6, 980.0), 320.0);
        assert!(layout.reset());
        assert_eq!(layout.resolved_height(RequestPane::Params, 6, 980.0), 544.0);

        assert_eq!(resizable_request_panel_height_bounds(980.0), (300.0, 706.0));
        assert_eq!(resizable_request_panel_height_bounds(500.0), (300.0, 300.0));
    }
}
