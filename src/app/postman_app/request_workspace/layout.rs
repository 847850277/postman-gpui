/// Resolved request viewport, shared with the editors that own row scrolling.
/// The split measures this region; window chrome is never subtracted a second time.
pub(super) struct RequestPanelLayout {
    height: f32,
    width: f32,
}
impl Default for RequestPanelLayout {
    fn default() -> Self {
        Self {
            height: 300.,
            width: 500.,
        }
    }
}
impl RequestPanelLayout {
    pub(super) fn show_descriptions(&self) -> bool {
        self.width >= 560.
    }
    pub(super) fn set_width(&mut self, width: f32) -> bool {
        if (self.width - width).abs() < 0.5 {
            return false;
        }
        self.width = width;
        true
    }
    pub(super) fn width(&self) -> f32 {
        self.width
    }
    pub(super) fn height(&self) -> f32 {
        self.height
    }
    pub(super) fn set_height(&mut self, height: f32) -> bool {
        if (self.height - height).abs() < 0.5 {
            return false;
        }
        self.height = height;
        true
    }
}
