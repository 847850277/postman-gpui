//! Design tokens mirrored from `design.pen`.

pub const BG: u32 = 0x00f4_f6f8;
pub const PANEL: u32 = 0x00ff_ffff;
pub const PANEL_ALT: u32 = 0x00f8_fafc;
pub const LINE: u32 = 0x00e2_e8f0;
pub const TEXT: u32 = 0x000f_172a;
pub const SUBTEXT: u32 = 0x0047_5569;
pub const MUTED: u32 = 0x0094_a3b8;
pub const ACCENT: u32 = 0x00f9_7316;
pub const ACCENT_DARK: u32 = 0x00c2_410c;
pub const ACCENT_SOFT: u32 = 0x00ff_f1e9;
pub const OK: u32 = 0x0016_a34a;
pub const ERROR: u32 = 0x00dc_2626;
pub const CODE_BG: u32 = 0x000f_172a;
pub const CODE_PANEL: u32 = 0x000b_1328;
pub const CODE_TEXT: u32 = 0x00ba_e6fd;

// The Pencil design uses Space Grotesk / Manrope / JetBrains Mono. They are not
// bundled with the app yet, so use installed macOS metric-compatible families to
// avoid invisible text when a requested family cannot be resolved.
pub const FONT_HEADING: &str = "Avenir Next";
pub const FONT_UI: &str = "Helvetica Neue";
pub const FONT_MONO: &str = "Menlo";
