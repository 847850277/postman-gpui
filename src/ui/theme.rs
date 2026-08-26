//! Design tokens mirrored from the Issue #51 Pencil artifact.

use crate::models::HttpMethod;

pub const BG: u32 = 0x00f4_f7f3;
pub const PANEL: u32 = 0x00ff_fefb;
pub const PANEL_ALT: u32 = 0x00ee_f5f1;
pub const LINE: u32 = 0x00cf_e0d7;
pub const TEXT: u32 = 0x0020_342b;
pub const SUBTEXT: u32 = 0x0052_6b60;
pub const MUTED: u32 = 0x0064_786e;
pub const ACCENT: u32 = 0x00c6_4b2b;
pub const ACCENT_VIVID: u32 = 0x00f5_6b3d;
pub const ACCENT_INK: u32 = 0x003c_1f16;
pub const ACCENT_DARK: u32 = 0x009f_371f;
pub const ACCENT_SOFT: u32 = 0x00ff_f0e8;
pub const OK: u32 = 0x000e_7a4e;
pub const OK_SOFT: u32 = 0x00e4_f6ea;
pub const INFO: u32 = 0x000f_718b;
pub const INFO_SOFT: u32 = 0x00e6_f4f7;
pub const ERROR: u32 = 0x00c2_3a3a;
pub const CODE_BG: u32 = PANEL;
pub const CODE_PANEL: u32 = PANEL_ALT;
pub const CODE_TEXT: u32 = 0x0024_3d34;

// These OFL-licensed families are embedded in the executable and registered at startup, so the
// Pencil typography contract does not depend on fonts installed by the host operating system.
pub const FONT_HEADING: &str = "Inter";
pub const FONT_UI: &str = "Inter";
pub const FONT_MONO: &str = "JetBrains Mono";

pub fn method_color(method: HttpMethod) -> u32 {
    match method {
        HttpMethod::GET => OK,
        HttpMethod::POST => ACCENT,
        HttpMethod::PUT => INFO,
        HttpMethod::DELETE => ERROR,
        HttpMethod::PATCH => 0x0072_51a3,
        HttpMethod::HEAD | HttpMethod::OPTIONS => SUBTEXT,
    }
}
