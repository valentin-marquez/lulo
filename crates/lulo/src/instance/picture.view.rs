//! An instance's picture and the controls that change it: upload one, pick at random, or
//! choose a game item and a color. The create dialog shows the short form; the instance's
//! settings show all of it.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Icon as Glyph, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{BACKGROUNDS, Icon, Instance, art};

const ART: f32 = 112.;

pub struct Picker {
    /// The picture so far: an item on a color, or an image picked from disk (absolute path
    /// until [`keep`] copies it into the instance).
    pub icon: Icon,
    /// The icon theme the palette shows.
    theme: usize,
    /// Whether the items and colors show under the picture.
    full: bool,
}

impl Picker {
    /// Starts from `icon`, or from a random one.
    pub fn new(icon: Option<Icon>, full: bool) -> Self {
        Self {
            icon: icon.unwrap_or_else(|| Icon::picked(&seed())),
            theme: 0,
            full,
        }
    }

    /// Asks the OS for a picture to use as the icon.
    fn upload(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use as icon".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.icon = Icon::Image {
                    file: path.to_string_lossy().into_owned(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The picture with Upload and Randomize beside it.
    fn preview(&self, cx: &mut Context<Self>) -> Div {
        let preview = Instance {
            memory_gb: None,
            jvm_args: String::new(),
            launch_args: String::new(),
            id: "preview".into(),
            dir: PathBuf::new(),
            name: "".into(),
            branch: "".into(),
            build: None,
            last_played: None,
            icon: Some(self.icon.clone()),
        };
        let button = |id: &'static str, icon: IconName, label: &'static str| {
            Button::new(id)
                .ghost()
                .cursor_pointer()
                .h(px(34.))
                .w(px(148.))
                .icon(icon)
                .label(label)
        };
        div()
            .flex()
            .items_center()
            .gap_3()
            // Icon swap: a new picture fades in rising 4 px (GPUI can't scale or blur a box).
            .child(
                art::art(&preview, ART, cx.theme().radius_lg, cx).with_animation(
                    ElementId::Name(format!("icon-{:?}", self.icon).into()),
                    Animation::new(crate::ui::ENTER).with_easing(crate::ui::ease_out_quint),
                    |art, t| art.relative().top(px(4. * (1. - t))).opacity(t),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        button("icon-upload", IconName::Download, "Upload")
                            .on_click(cx.listener(|this, _, _, cx| this.upload(cx))),
                    )
                    .child(
                        button("icon-random", IconName::LoaderCircle, "Randomize").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.icon = Icon::picked(&seed());
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }

    /// The items and colors Customize offers.
    fn palette(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let (item, color) = match &self.icon {
            Icon::Item { item, color } => (item.clone(), *color),
            Icon::Image { .. } => (String::new(), BACKGROUNDS[0].0),
        };
        let (_, shown) = crate::game::THEMES[self.theme];
        let tabs = TabBar::new("icon-themes")
            .segmented()
            .small()
            .selected_index(self.theme)
            .children(
                crate::game::THEMES
                    .iter()
                    .map(|(name, _)| Tab::new().label(*name).cursor_pointer()),
            )
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.theme = *ix;
                cx.notify();
            }));
        let items = shown.iter().map(|&pick| {
            let selected = pick == item;
            div()
                .id(SharedString::from(format!("pick-{pick}")))
                .flex()
                .items_center()
                .justify_center()
                .size(px(44.))
                .rounded(theme.radius)
                .cursor_pointer()
                .border_1()
                .map(|this| {
                    if selected {
                        this.bg(theme.primary.opacity(0.25))
                            .border_color(theme.primary)
                    } else {
                        this.border_color(transparent_black())
                            .hover(|s| s.bg(theme.muted))
                    }
                })
                .child(img(SharedString::from(crate::game::icon_path(pick))).size(px(32.)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.icon = Icon::Item {
                        item: pick.to_owned(),
                        color,
                    };
                    cx.notify();
                }))
        });
        // Rounded squares with the icon's own gradient; the chosen one carries a check.
        let colors = BACKGROUNDS.iter().map(|&(swatch, bottom)| {
            let selected = swatch == color && !item.is_empty();
            let item = item.clone();
            div()
                .id(SharedString::from(format!("color-{swatch:06x}")))
                .relative()
                .size(px(40.))
                .rounded(theme.radius)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(rgb(swatch), 0.),
                    linear_color_stop(rgb(bottom), 1.),
                ))
                .border_1()
                .border_color(if selected {
                    theme.foreground
                } else {
                    black().opacity(0.1)
                })
                .cursor_pointer()
                .when(selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top_1()
                            .right_1()
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(white().opacity(0.85))
                            .text_color(black())
                            .child(Glyph::new(IconName::Check).size(px(10.))),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let item = if item.is_empty() {
                        shown[0].to_owned()
                    } else {
                        item.clone()
                    };
                    this.icon = Icon::Item {
                        item,
                        color: swatch,
                    };
                    cx.notify();
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .rounded(theme.radius_lg)
            .bg(theme.muted)
            .child(tabs)
            .child(div().flex().flex_wrap().gap_1().children(items))
            .child(div().flex().flex_wrap().gap_2().children(colors))
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.preview(cx))
            .when(self.full, |this| this.child(self.palette(cx)))
    }
}

/// A seed that changes on every call, for Randomize.
fn seed() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_nanos().to_string()
}

/// The icon to store: a picked picture is copied into the instance as `icon.<ext>`.
pub fn keep(icon: &Icon, dir: &Path) -> std::io::Result<Icon> {
    let Icon::Image { file } = icon else {
        return Ok(icon.clone());
    };
    let from = Path::new(file);
    let ext = from
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("png");
    let name = format!("icon.{ext}");
    std::fs::copy(from, dir.join(&name))?;
    Ok(Icon::Image { file: name })
}
