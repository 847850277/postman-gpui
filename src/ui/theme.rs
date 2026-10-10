//! Product tokens from prototypes/request-workspace.html at ec5c09a (#196).
//! Kit owns the active mode; legacy controls resolve the same tokens at render time.

use crate::models::HttpMethod;
use gpui::{px, App, Rgba};
use gpui_kit::component::{Theme, ThemeMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorToken {
    light: u32,
    dark: u32,
}
impl ColorToken {
    pub const fn new(light: u32, dark: u32) -> Self {
        Self { light, dark }
    }
    pub fn for_mode(self, mode: ThemeMode) -> Rgba {
        gpui::rgb(if mode.is_dark() {
            self.dark
        } else {
            self.light
        })
    }
    pub fn resolve(self, cx: &App) -> Rgba {
        self.for_mode(
            cx.try_global::<Theme>()
                .map_or(ThemeMode::Light, |t| t.mode),
        )
    }
}

pub const BG: ColorToken = ColorToken::new(0xf3f3f2, 0x18191b);
pub const PANEL: ColorToken = ColorToken::new(0xffffff, 0x1d1e20);
pub const SIDEBAR: ColorToken = ColorToken::new(0xf8f8f7, 0x191a1c);
pub const PANEL_ALT: ColorToken = ColorToken::new(0xf5f5f4, 0x232426);
pub const ELEVATED: ColorToken = ColorToken::new(0xffffff, 0x28292c);
pub const LINE: ColorToken = ColorToken::new(0xe2e2df, 0x303135);
pub const LINE_STRONG: ColorToken = ColorToken::new(0xc2c2bd, 0x53555b);
pub const TEXT: ColorToken = ColorToken::new(0x242523, 0xf0f0ee);
pub const SUBTEXT: ColorToken = ColorToken::new(0x62645f, 0xacaeb4);
pub const MUTED: ColorToken = ColorToken::new(0x686b64, 0x92959e);
pub const ACCENT: ColorToken = ColorToken::new(0xac411b, 0xff9a6c);
pub const ACCENT_HOVER: ColorToken = ColorToken::new(0x913313, 0xffb18d);
pub const ACCENT_SOFT: ColorToken = ColorToken::new(0xfbede6, 0x382a25);
pub const ON_ACCENT: ColorToken = ColorToken::new(0xffffff, 0x311809);
pub const OK: ColorToken = ColorToken::new(0x237246, 0x83d5a1);
pub const OK_SOFT: ColorToken = ColorToken::new(0xeaf5ed, 0x23352c);
pub const INFO: ColorToken = ColorToken::new(0x386bc1, 0x91b7f3);
pub const INFO_SOFT: ColorToken = ColorToken::new(0xeaf0fc, 0x252f40);
pub const ERROR: ColorToken = ColorToken::new(0xba3b43, 0xf5939b);
pub const ERROR_SOFT: ColorToken = ColorToken::new(0xfcebed, 0x3c272b);
pub const PURPLE: ColorToken = ColorToken::new(0x7751ad, 0xbea2ec);
pub const CODE_STRING: ColorToken = ColorToken::new(0x287549, 0xadd49d);
pub const CODE_KEY: ColorToken = ColorToken::new(0x505c70, 0xc0c8d6);
pub const CODE_NUMBER: ColorToken = ColorToken::new(0xae5426, 0xe9b583);
// Compatibility names keep existing controls on the same semantic palette during P2–P4.
pub const ACCENT_VIVID: ColorToken = ACCENT;
pub const ACCENT_INK: ColorToken = ON_ACCENT;
pub const ACCENT_DARK: ColorToken = ACCENT;
pub const CODE_BG: ColorToken = PANEL_ALT;
pub const CODE_PANEL: ColorToken = PANEL_ALT;
pub const CODE_TEXT: ColorToken = TEXT;
pub const FONT_HEADING: &str = "Inter";
pub const FONT_UI: &str = "Inter";
pub const FONT_MONO: &str = "JetBrains Mono";

/// All product lengths are relative to the 16px Kit rem base. The prototype's
/// 13px body type is a separate role, so control geometry and type zoom together.
pub mod metrics {
    pub const MEDIUM: gpui::FontWeight = gpui::FontWeight::MEDIUM;
    pub const SEMIBOLD: gpui::FontWeight = gpui::FontWeight::SEMIBOLD;
    use gpui::{rems, Rems};
    pub const BODY: Rems = rems(13. / 16.);
    pub const LABEL: Rems = rems(12. / 16.);
    pub const CAPTION: Rems = rems(10. / 16.);
    pub const CODE: Rems = rems(12. / 16.);
    pub const TITLE: Rems = rems(26. / 16.);
    pub const LINE_HEIGHT: f32 = 1.5;
    pub const CODE_LINE_HEIGHT: f32 = 1.9;
    pub const CONTROL: Rems = rems(36. / 16.);
    pub const URL: Rems = rems(48. / 16.);
    pub const ICON_BUTTON: Rems = rems(32. / 16.);
    pub const ICON: Rems = rems(18. / 16.);
    pub const SMALL_ICON: Rems = rems(15. / 16.);
    pub const TABLE_ROW: Rems = rems(40. / 16.);
    pub const TABLE_HEADER: Rems = rems(32. / 16.);
    pub const PANE_TAB: Rems = rems(43. / 16.);
    pub const REQUEST_TAB: Rems = rems(46. / 16.);
    pub const RAIL: Rems = rems(72. / 16.);
    pub const TITLEBAR: Rems = rems(52. / 16.);
    pub const STATUSBAR: Rems = rems(30. / 16.);
    pub const RADIUS: Rems = rems(6. / 16.);
    pub const URL_RADIUS: Rems = rems(7. / 16.);
    pub const DIALOG_RADIUS: Rems = rems(12. / 16.);
    pub const DIALOG_WIDTH: Rems = rems(560. / 16.);
    pub const FIELD_INSET: Rems = rems(10. / 16.);
    pub const URL_INSET: Rems = rems(15. / 16.);
    pub const METHOD_WIDTH: Rems = rems(110. / 16.);
}

pub fn method_color(method: HttpMethod) -> ColorToken {
    match method {
        HttpMethod::GET | HttpMethod::HEAD | HttpMethod::OPTIONS => OK,
        HttpMethod::POST => ACCENT,
        HttpMethod::PUT | HttpMethod::PATCH => INFO,
        HttpMethod::DELETE => ERROR,
    }
}

/// Update through Kit so styled tokens, Base overlays and all windows stay synchronized.
pub fn apply(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);
    let c = |token: ColorToken| token.for_mode(mode).into();
    Theme::update(cx, |t| {
        t.font_family = FONT_UI.into();
        t.mono_font_family = FONT_MONO.into();
        t.font_size = px(16.);
        t.mono_font_size = px(12.);
        t.radius = px(6.);
        t.radius_lg = px(12.);
        t.background = c(PANEL);
        t.foreground = c(TEXT);
        t.border = c(LINE);
        t.input = c(LINE_STRONG);
        t.ring = c(ACCENT);
        t.caret = c(TEXT);
        t.selection = c(ACCENT_SOFT);
        t.muted = c(PANEL_ALT);
        t.muted_foreground = c(MUTED);
        t.accent = c(PANEL_ALT);
        t.accent_foreground = c(TEXT);
        t.primary = c(ACCENT);
        t.primary_hover = c(ACCENT_HOVER);
        t.primary_active = c(ACCENT_HOVER);
        t.primary_foreground = c(ON_ACCENT);
        t.secondary = c(PANEL_ALT);
        t.secondary_hover = c(ELEVATED);
        t.secondary_active = c(ACCENT_SOFT);
        t.secondary_foreground = c(SUBTEXT);
        t.button = c(PANEL_ALT);
        t.button_hover = c(ELEVATED);
        t.button_active = c(ACCENT_SOFT);
        t.button_foreground = c(TEXT);
        t.button_primary = c(ACCENT);
        t.button_primary_hover = c(ACCENT_HOVER);
        t.button_primary_active = c(ACCENT_HOVER);
        t.button_primary_foreground = c(ON_ACCENT);
        t.danger = c(ERROR);
        t.danger_hover = c(ERROR);
        t.danger_active = c(ERROR);
        t.danger_foreground = c(PANEL);
        t.success = c(OK);
        t.success_foreground = c(PANEL);
        t.info = c(INFO);
        t.info_foreground = c(PANEL);
        t.popover = c(ELEVATED);
        t.popover_foreground = c(TEXT);
        t.group_box = c(PANEL);
        t.group_box_foreground = c(TEXT);
        t.sidebar = c(SIDEBAR);
        t.sidebar_foreground = c(TEXT);
        t.sidebar_border = c(LINE);
        t.sidebar_accent = c(ACCENT_SOFT);
        t.sidebar_accent_foreground = c(ACCENT);
        t.title_bar = c(BG);
        t.title_bar_border = c(LINE);
        t.status_bar = c(BG);
        t.status_bar_border = c(LINE);
        t.tab = c(PANEL);
        t.tab_bar = c(PANEL);
        t.tab_active = c(PANEL);
        t.tab_foreground = c(SUBTEXT);
        t.tab_active_foreground = c(ACCENT);
        t.table = c(PANEL);
        t.table_head = c(PANEL_ALT);
        t.table_head_foreground = c(MUTED);
        t.table_row_border = c(LINE);
        t.table_even = c(PANEL);
        t.table_hover = c(PANEL_ALT);
        t.table_active = c(ACCENT_SOFT);
        t.table_active_border = c(ACCENT);
        t.colors.list = c(PANEL);
        t.list_hover = c(PANEL_ALT);
        t.list_active = c(ACCENT_SOFT);
        t.list_active_border = c(ACCENT);
        t.list_head = c(PANEL_ALT);
        t.scrollbar_thumb = c(LINE_STRONG);
        t.scrollbar_thumb_hover = c(MUTED);
        t.drag_border = c(ACCENT);
        t.link = c(INFO);
        t.overlay = gpui::rgba(0x08090b99).into();
        let highlight = std::sync::Arc::make_mut(&mut t.highlight_theme);
        highlight.style.editor_background = Some(c(PANEL));
        highlight.style.editor_foreground = Some(c(TEXT));
        highlight.style.editor_active_line = Some(c(PANEL_ALT));
        highlight.style.editor_line_number = Some(c(MUTED));
        highlight.style.editor_active_line_number = Some(c(SUBTEXT));
        highlight.style.editor_gutter_background = Some(c(PANEL));
        let syntax = |token| {
            serde_json::from_value(serde_json::json!({"color": c(token)}))
                .expect("valid semantic syntax color")
        };
        highlight.style.syntax.string = Some(syntax(CODE_STRING));
        highlight.style.syntax.property = Some(syntax(CODE_KEY));
        highlight.style.syntax.number = Some(syntax(CODE_NUMBER));
        highlight.style.syntax.boolean = Some(syntax(PURPLE));
        t.motion.duration_fast = std::time::Duration::from_millis(140);
    });
}

/// HTML's modal elevation; stored here alongside the semantic palette.
pub fn dialog_shadow(cx: &App) -> Vec<gpui::BoxShadow> {
    let dark = cx.try_global::<Theme>().is_some_and(|t| t.mode.is_dark());
    vec![gpui::BoxShadow {
        inset: false,
        color: gpui::rgba(if dark { 0x00000077 } else { 0x20231b26 }).into(),
        offset: gpui::point(px(0.), px(20.)),
        blur_radius: px(80.),
        spread_radius: px(0.),
    }]
}
