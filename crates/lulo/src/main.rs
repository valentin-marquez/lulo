mod account;
mod assets;
mod instance;
mod library;
mod runtime;
mod screenshots;
mod shell;
mod theme;
mod ui;

use std::sync::Arc;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

use crate::assets::AppAssets;
use crate::shell::Shell;

// Must match the .desktop file name so Wayland compositors resolve the icon.
const APP_ID: &str = "io.github.valentin_marquez.Lulo";
const ICON_PNG: &[u8] = include_bytes!("../../../assets/icons/png/lulo-256.png");

fn window_icon() -> Option<Arc<image::RgbaImage>> {
    image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .ok()
        .map(|img| Arc::new(img.into_rgba8()))
}

fn main() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);

        let options = WindowOptions {
            app_id: Some(APP_ID.into()),
            icon: window_icon(),
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
            window_min_size: Some(size(px(900.), px(600.))),
            titlebar: Some(TitlebarOptions {
                // Centre macOS traffic lights in the taller title bar.
                traffic_light_position: Some(point(px(16.), px(17.))),
                ..TitleBar::title_bar_options()
            }),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            theme::follow_system(window, cx);
            cx.new(Shell::new)
        })
        .expect("failed to open main window");
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icon_decodes() {
        let icon = super::window_icon().expect("icon must decode");
        assert_eq!(icon.dimensions(), (256, 256));
    }
}
