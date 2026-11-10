//! Asset source: the component icon set, the extra Lucide icons we use, and the logo.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::*;

icon_assets!(Icons, [Box, Compass, Puzzle]);

pub const LOGO: &str = "brand/lulo-64.png";

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/png/lulo-64.png"
            ))));
        }
        if let Some(bytes) = Icons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(Icons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
