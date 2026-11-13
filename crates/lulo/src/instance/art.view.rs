//! An instance's picture at any size: a game item on its color, the user's own image, or the
//! plain box for an item no longer in the set.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon as Glyph};
use gpui_kit::*;

use super::{Icon, Instance};

/// `size` is the square's side; `radius` its corners.
pub fn art(instance: &Instance, size: f32, radius: Pixels, cx: &App) -> Div {
    let theme = cx.theme();
    let frame = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(radius)
        .overflow_hidden();
    match instance.icon() {
        Icon::Image { file } => frame.bg(theme.muted).child(
            img(instance.dir.join(file))
                .size_full()
                .object_fit(ObjectFit::Cover),
        ),
        Icon::Item { item, color } => match crate::game::icon(&item) {
            Some(_) => {
                let bottom = super::background_bottom(color);
                frame
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(rgb(color), 0.),
                        linear_color_stop(rgb(bottom), 1.),
                    ))
                    .child(
                        img(SharedString::from(crate::game::icon_path(&item)))
                            .size(px(sprite(size))),
                    )
            }
            _ => placeholder(frame, size, cx),
        },
    }
}

/// Item icons are 32 px pixel art: whole multiples of 32 keep every pixel square.
fn sprite(size: f32) -> f32 {
    let whole = (size * 0.85 / 32.).floor() * 32.;
    if whole >= 32. { whole } else { size * 0.8 }
}

fn placeholder(frame: Div, size: f32, cx: &App) -> Div {
    let theme = cx.theme();
    frame
        .bg(theme.muted)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.muted_foreground)
        .child(Glyph::new(IconName::Box).size(px(size * 0.4)))
}

#[cfg(test)]
mod tests {
    use super::sprite;

    #[test]
    fn keeps_pixel_art_on_whole_multiples() {
        assert_eq!(sprite(160.), 128.);
        assert_eq!(sprite(56.), 32.);
        assert_eq!(sprite(34.), 34. * 0.8, "too small for 32: scale to fit");
    }
}
