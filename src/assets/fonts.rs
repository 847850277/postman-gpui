//! Fonts embedded in the executable for consistent rendering on every platform.

use gpui::App;
use std::borrow::Cow;

static SPACE_GROTESK: &[u8] =
    include_bytes!("../../assets/fonts/space-grotesk/SpaceGrotesk[wght].ttf");
static MANROPE: &[u8] = include_bytes!("../../assets/fonts/manrope/Manrope[wght].ttf");
static JETBRAINS_MONO: &[u8] =
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono[wght].ttf");

const EMBEDDED_FONT_FAMILIES: [&str; 3] = ["Space Grotesk", "Manrope", "JetBrains Mono"];

/// Registers the bundled UI fonts before any windows are created.
pub fn load_embedded_fonts(cx: &App) -> anyhow::Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(SPACE_GROTESK),
        Cow::Borrowed(MANROPE),
        Cow::Borrowed(JETBRAINS_MONO),
    ])
}

/// Confirms that the active native text backend resolved every embedded family.
pub fn verify_embedded_fonts(cx: &App) -> anyhow::Result<()> {
    let available_fonts = cx.text_system().all_font_names();
    for expected_family in EMBEDDED_FONT_FAMILIES {
        anyhow::ensure!(
            available_fonts
                .iter()
                .any(|family| family == expected_family),
            "embedded font family {expected_family:?} was not registered"
        );
    }
    Ok(())
}
