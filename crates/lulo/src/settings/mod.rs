//! The settings dialog, and the settings kept on disk: sections on the left, the selected one
//! on the right.

#[path = "startup.model.rs"]
mod startup;

use std::path::PathBuf;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde::{Deserialize, Serialize};

use crate::theme::{self, Mode};
use crate::ui::FancyButton;

/// What is saved in `settings.json`, next to the instances folder.
#[derive(Default, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub theme: Mode,
}

fn file() -> PathBuf {
    crate::instance::store::root().with_file_name("settings.json")
}

/// The saved settings; a missing or unreadable file means the defaults.
pub fn load() -> Saved {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Changes the saved settings.
pub fn update(change: impl FnOnce(&mut Saved)) {
    let mut saved = load();
    change(&mut saved);
    let Ok(text) = serde_json::to_string_pretty(&saved) else {
        return;
    };
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Appearance,
    Behavior,
}

impl Section {
    const ALL: [Section; 2] = [Section::Appearance, Section::Behavior];

    fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Behavior => "Behavior",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Section::Appearance => IconName::Palette,
            Section::Behavior => IconName::Settings2,
        }
    }
}

const WIDTH: f32 = 860.;
const HEIGHT: f32 = 440.;
const NAV: f32 = 220.;
/// The height of a theme card's preview.
const PREVIEW: f32 = 96.;
/// How far below its place the unsaved-changes bar starts: its height plus margin, so it begins
/// out of sight.
const BAR_TRAVEL: f32 = 80.;

pub fn open(window: &mut Window, cx: &mut App) {
    use gpui_kit::component::WindowExt;
    let now = Values {
        theme: theme::mode(cx),
        startup: startup::enabled(),
    };
    let view = cx.new(|_| Settings {
        section: Section::Appearance,
        saved: now,
        draft: now,
        was_dirty: false,
        left_at: None,
    });
    // However the dialog closes (X, Escape, outside click), the theme goes back to the saved one.
    cx.observe_release(&view, |settings, cx| {
        theme::preview(settings.saved.theme, None, cx)
    })
    .detach();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .entrance(DialogEntrance::FadeSlide)
            .title("Settings")
            .centered()
            .w(px(WIDTH))
            .child(view.clone())
    });
}

/// The settings the dialog edits.
#[derive(Clone, Copy, PartialEq)]
struct Values {
    theme: Mode,
    /// Whether the system starts lulo at sign in.
    startup: bool,
}

struct Settings {
    section: Section,
    /// What is in effect now.
    saved: Values,
    /// What the dialog shows. The theme is previewed as it is picked; only Save keeps it, and
    /// closing the dialog goes back to `saved`.
    draft: Values,
    /// Whether the last frame had unsaved changes, to notice when they stop.
    was_dirty: bool,
    /// When the changes stopped: the bar keeps drawing for a moment while it leaves.
    left_at: Option<Instant>,
}

/// The few colors of a theme preview, fixed so each card shows its own theme.
struct Palette {
    page: u32,
    panel: u32,
    tile: u32,
    strong: u32,
    soft: u32,
}

const DARK: Palette = Palette {
    page: 0x111113,
    panel: 0x1c1c1f,
    tile: 0x2a2a2e,
    strong: 0xd4d4d8,
    soft: 0x71717a,
};

const LIGHT: Palette = Palette {
    page: 0xf4f4f5,
    panel: 0xffffff,
    tile: 0xe4e4e7,
    strong: 0x27272a,
    soft: 0xa1a1aa,
};

/// A tiny window on a page: a tile and two lines on a panel. `compact` is for the half cards of
/// System, which have room for less.
fn mock(p: &Palette, compact: bool) -> Div {
    let (tile, long, short) = if compact {
        (18., 26., 16.)
    } else {
        (24., 56., 36.)
    };
    let line = |width: f32, color: u32| div().h(px(6.)).w(px(width)).rounded_full().bg(rgb(color));
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .h(px(PREVIEW))
        .bg(rgb(p.page))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .p_2()
                .rounded(px(8.))
                .bg(rgb(p.panel))
                .child(div().size(px(tile)).rounded(px(6.)).bg(rgb(p.tile)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(line(long, p.strong))
                        .child(line(short, p.soft)),
                ),
        )
}

/// The top of a theme card. The card's own clip is a plain rectangle, so the preview rounds
/// its top corners itself, one pixel inside the card's radius for its border.
fn preview(mode: Mode, radius: Pixels) -> Div {
    let top = radius - px(1.);
    match mode {
        Mode::Dark => mock(&DARK, false).rounded_t(top),
        Mode::Light => mock(&LIGHT, false).rounded_t(top),
        // Half dark, half light, side by side.
        Mode::System => div()
            .flex()
            .child(mock(&DARK, true).rounded_tl(top))
            .child(mock(&LIGHT, true).rounded_tr(top)),
    }
}

fn radio(selected: bool, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(14.))
        .rounded_full()
        .border_1()
        .border_color(if selected {
            theme.primary
        } else {
            theme.muted_foreground
        })
        .children(selected.then(|| div().size(px(8.)).rounded_full().bg(theme.primary)))
}

fn heading(title: &'static str, line: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_lg().font_weight(FontWeight::BOLD).child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(line),
        )
}

/// A setting with its explanation on the left and its control on the right.
fn row(title: &'static str, line: &'static str, control: impl IntoElement, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_6()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(line),
                ),
        )
        .child(control)
}

impl Settings {
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_kit::component::WindowExt;
        if self.draft.startup != self.saved.startup {
            if let Err(err) = startup::set(self.draft.startup) {
                window.push_notification(format!("Couldn't change that: {err}"), cx);
                return;
            }
            self.saved.startup = self.draft.startup;
        }
        if self.draft.theme != self.saved.theme {
            theme::choose(self.draft.theme, window, cx);
            self.saved.theme = self.draft.theme;
        }
        cx.notify();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = self.saved;
        theme::preview(self.saved.theme, Some(window), cx);
        cx.notify();
    }

    /// The bar that asks to keep or drop what was changed.
    fn unsaved(&self, leaving: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .mx_6()
            .mb_4()
            .px_4()
            .py_3()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("You have unsaved changes."),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("reset")
                            .ghost()
                            .cursor_pointer()
                            .icon(IconName::Undo2)
                            .label("Reset")
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    )
                    .child(
                        FancyButton::new("save", "Save")
                            .small()
                            .icon(IconName::Check)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
            // Rises into view from below the dialog's edge (the panel clips what is still under it)
            // and sinks back the same way when the changes are gone.
            .with_animation(
                if leaving {
                    "unsaved-leave"
                } else {
                    "unsaved-enter"
                },
                Animation::new(crate::ui::ENTER).with_easing(crate::ui::ease_out_quint),
                move |bar, t| {
                    let shown = if leaving { 1. - t } else { t };
                    bar.relative()
                        .top(px(BAR_TRAVEL * (1. - shown)))
                        .opacity(shown)
                },
            )
    }

    fn nav(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .w(px(NAV))
            .p_3()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_3()
                    .pt_1()
                    .pb_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("DISPLAY"),
            )
            .children(Section::ALL.map(|section| {
                let active = section == self.section;
                div()
                    .id(section.label())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius)
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .when(active, |this| {
                        this.bg(theme.primary.opacity(0.15))
                            .text_color(theme.primary)
                    })
                    .when(!active, |this| {
                        this.text_color(theme.muted_foreground)
                            .hover(|style| style.bg(theme.sidebar_accent))
                    })
                    .child(Icon::new(section.icon()).size_4())
                    .child(section.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    }))
            }))
    }

    fn appearance(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let current = self.draft.theme;
        let cards = Mode::ALL.map(|mode| {
            let selected = mode == current;
            div()
                .id(mode.label())
                .flex()
                .flex_1()
                .min_w_0()
                .flex_col()
                .rounded(theme.radius_lg)
                .overflow_hidden()
                .border_1()
                .border_color(if selected {
                    theme.primary
                } else {
                    theme.border
                })
                .cursor_pointer()
                .child(preview(mode, theme.radius_lg))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .rounded_b(theme.radius_lg - px(1.))
                        .bg(theme.group_box)
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(radio(selected, cx))
                        .child(mode.label()),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.draft.theme = mode;
                    theme::preview(mode, Some(window), cx);
                    cx.notify();
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(heading(
                "Color theme",
                "System follows your system's light or dark setting.",
                cx,
            ))
            .child(div().flex().gap_3().children(cards))
    }

    fn behavior(&self, cx: &mut Context<Self>) -> Div {
        let this = cx.entity();
        let startup =
            Switch::new("startup")
                .checked(self.draft.startup)
                .on_click(move |&on, _, cx| {
                    this.update(cx, |this, cx| {
                        this.draft.startup = on;
                        cx.notify();
                    })
                });
        // Needs a system tray icon, which gpui doesn't have.
        let tray = Switch::new("tray").checked(false).disabled(true);
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(heading(
                "Behavior",
                "How lulo starts and what closing it does.",
                cx,
            ))
            .child(row(
                "Launch at startup",
                "Start lulo when you sign in to your computer.",
                startup,
                cx,
            ))
            .child(row(
                "Minimize to the system tray",
                "Closing the window keeps lulo running in the tray. Coming soon.",
                tray,
                cx,
            ))
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let content = match self.section {
            Section::Appearance => self.appearance(cx),
            Section::Behavior => self.behavior(cx),
        };
        let dirty = self.draft != self.saved;
        if self.was_dirty && !dirty {
            self.left_at = Some(Instant::now());
            // Draw once more when the bar has finished leaving, so it goes away.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(crate::ui::ENTER).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        self.was_dirty = dirty;
        let leaving = !dirty
            && self
                .left_at
                .is_some_and(|at| at.elapsed() < crate::ui::ENTER);
        let unsaved = (dirty || leaving).then(|| self.unsaved(leaving, cx));
        div()
            .flex()
            .h(px(HEIGHT))
            .border_t_1()
            .border_color(border)
            .child(self.nav(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("settings-content")
                            .flex_1()
                            .min_h_0()
                            .p_6()
                            .overflow_y_scroll()
                            .child(content),
                    )
                    .children(unsaved),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Saved};

    #[test]
    fn saved_settings_read_back_and_default() {
        let read = |text: &str| serde_json::from_str::<Saved>(text).map(|s| s.theme);
        assert!(read(r#"{"theme":"dark"}"#).unwrap() == Mode::Dark);
        assert!(read(r#"{"theme":"light"}"#).unwrap() == Mode::Light);
        assert!(read("{}").unwrap() == Mode::System);
        assert!(read(r#"{"theme":"purple"}"#).is_err());
        let written = serde_json::to_string(&Saved { theme: Mode::Light }).unwrap();
        assert_eq!(written, r#"{"theme":"light"}"#);
    }
}
