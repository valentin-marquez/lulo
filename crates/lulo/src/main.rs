use std::sync::Arc;

use gpui_kit::base::StyledExt;
use gpui_kit::*;

// Must match the .desktop file name so Wayland compositors resolve the icon.
const APP_ID: &str = "io.github.valentin_marquez.Lulo";
const ICON_PNG: &[u8] = include_bytes!("../../../assets/icons/png/lulo-256.png");

struct Launcher;

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child("lulo")
    }
}

fn window_icon() -> Option<Arc<image::RgbaImage>> {
    image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .ok()
        .map(|img| Arc::new(img.into_rgba8()))
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                app_id: Some(APP_ID.into()),
                icon: window_icon(),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Launcher))
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
