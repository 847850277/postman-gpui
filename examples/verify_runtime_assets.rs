//! Verifies that the native GPUI text backend can load every embedded font without opening a UI.

use gpui::App;
use postman_gpui::assets::fonts::{load_embedded_fonts, verify_embedded_fonts};

fn main() {
    gpui_platform::headless().run(|cx: &mut App| {
        load_embedded_fonts(cx).expect("failed to register embedded fonts");
        verify_embedded_fonts(cx).expect("native text backend did not resolve embedded fonts");

        println!("embedded runtime fonts loaded successfully");
        cx.quit();
    });
}
