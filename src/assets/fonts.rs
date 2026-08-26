//! Fonts embedded in the executable for consistent rendering on every platform.

use gpui::App;
use std::borrow::Cow;

static SPACE_GROTESK: &[u8] =
    include_bytes!("../../assets/fonts/space-grotesk/SpaceGrotesk[wght].ttf");
static MANROPE: &[u8] = include_bytes!("../../assets/fonts/manrope/Manrope[wght].ttf");
static JETBRAINS_MONO: &[u8] =
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono[wght].ttf");

const EMBEDDED_FONT_FAMILIES: [&str; 3] = ["Space Grotesk", "Manrope", "JetBrains Mono"];

/// Creates a no-window application with a real text backend for package verification.
///
/// GPUI's Windows headless platform deliberately uses `NoopTextSystem`, so DirectWrite font
/// registration can only be verified through the normal platform initialization. Linux headless
/// mode retains the Cosmic Text backend when the Wayland/X11 features are enabled, and macOS
/// retains its font-kit backend.
pub fn runtime_asset_application() -> gpui::Application {
    #[cfg(target_os = "windows")]
    {
        gpui_platform::application()
    }

    #[cfg(not(target_os = "windows"))]
    {
        gpui_platform::headless()
    }
}

/// Schedules a successful verifier shutdown after the native event loop starts.
///
/// Linux invokes the application startup callback before entering its calloop event loop. Calling
/// `App::quit` directly from that callback stops a loop that has not started yet, after which the
/// newly started loop waits forever. A foreground task is first polled by the running event loop,
/// so quitting from that task shuts every platform down normally.
pub fn schedule_runtime_asset_exit(cx: &App) {
    cx.spawn(async |cx| {
        cx.update(|cx| cx.quit());
    })
    .detach();
}

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
