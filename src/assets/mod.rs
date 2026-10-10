pub mod fonts;

// Keep Kit's default icon set and embed the additional product icons.
gpui_kit::assets::icon_assets!(
    RequestIcons,
    [
        Lock, House, Terminal, Workflow, Cookie, Keyboard, RotateCcw, Plus, Search, ArrowRight, X,
        Send, Rows2, Columns2, Trash, Code, Upload
    ]
);
pub struct KitAssets;
impl gpui::AssetSource for KitAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match RequestIcons.load(path)? {
            Some(asset) => Ok(Some(asset)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }
    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(RequestIcons.list(path)?);
        Ok(paths)
    }
}
