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
        | RequestPane::Cookies
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
        | RequestPane::Cookies
        | RequestPane::Scripts
        | RequestPane::Tests
        | RequestPane::Options => return 0,
    };
    let additional_rows = ((panel_height - REQUEST_PANEL_BASE_HEIGHT) / PARAM_ROW_PITCH)
        .max(0.0)
        .floor() as usize;
    (PARAM_ROWS_AT_BASE_HEIGHT + additional_rows).min(max_visible_rows)
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
        adaptive_request_panel_height, row_scrollbar_geometry, visible_row_capacity,
        RowScrollbarGeometry,
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
    }
}
