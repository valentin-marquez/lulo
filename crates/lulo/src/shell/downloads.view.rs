//! Download manager: a pill in the title bar whose fill is the overall progress, opening a
//! panel with every install (bar, size, speed, time left, cancel). Modrinth App's pattern.

use gpui_kit::assets::IconName;
use gpui_kit::base::Selectable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::*;

use super::Shell;
use crate::instance::install::{self, Download};

/// What the pill shows; `None` hides it (nothing downloading).
#[derive(IntoElement)]
pub struct Pill {
    summary: Option<Summary>,
    shell: WeakEntity<Shell>,
}

struct Summary {
    /// The first install's name, or how many there are.
    title: SharedString,
    /// Overall, 0 to 1.
    fraction: f32,
    pct: u8,
    speed: u64,
}

impl Pill {
    pub fn new(shell: &Shell, handle: WeakEntity<Shell>) -> Self {
        let all = shell.downloads();
        let summary = (!all.is_empty()).then(|| {
            let done: u64 = all.iter().map(|(_, d)| d.done).sum();
            let total: u64 = all.iter().map(|(_, d)| d.total).sum();
            let pct = (done * 100).checked_div(total).unwrap_or(0).min(99) as u8;
            Summary {
                title: match all.as_slice() {
                    [(_, only)] => only.name.clone(),
                    many => format!("{} downloads", many.len()).into(),
                },
                fraction: pct as f32 / 100.,
                pct,
                speed: all.iter().map(|(_, d)| d.speed).sum(),
            }
        });
        Self {
            summary,
            shell: handle,
        }
    }
}

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Some(summary) = self.summary else {
            return div().into_any_element();
        };
        let shell = self.shell;
        Popover::new("downloads")
            .anchor(Anchor::TopRight)
            .trigger(Trigger {
                summary,
                open: false,
            })
            .content(move |_, _, cx| panel(&shell, cx))
            .into_any_element()
    }
}

/// The pill itself: a Popover trigger has to be `Selectable`.
#[derive(IntoElement)]
struct Trigger {
    summary: Summary,
    open: bool,
}

impl Selectable for Trigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let s = self.summary;
        div()
            .id("downloads-pill")
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .max_w(px(320.))
            .px_3()
            .rounded_full()
            .border_1()
            .border_color(if self.open {
                theme.primary
            } else {
                theme.border
            })
            .text_xs()
            .cursor_pointer()
            .hover(|style| style.border_color(theme.primary))
            // The fill behind the content is the progress bar.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(relative(s.fraction))
                    .bg(theme.primary.opacity(0.18)),
            )
            .child(
                Icon::new(IconName::Download)
                    .size_3p5()
                    .text_color(theme.primary),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(s.title),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(theme.muted_foreground)
                    .child(format!("{}%", s.pct)),
            )
            .children((s.speed > 0).then(|| {
                div()
                    .flex_none()
                    .text_color(theme.muted_foreground)
                    .child(format!("{}/s", install::size(s.speed)))
            }))
    }
}

/// Every install in progress. Rendered on each frame the popover is open, so it stays live.
fn panel(shell: &WeakEntity<Shell>, cx: &mut App) -> AnyElement {
    let Some(entity) = shell.upgrade() else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let rows: Vec<AnyElement> = entity
        .read(cx)
        .downloads()
        .into_iter()
        .map(|(id, download)| row(id.clone(), download, shell.clone(), &theme))
        .collect();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .w(px(360.))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Downloads"),
        )
        .children(rows)
        .into_any_element()
}

fn row(
    id: SharedString,
    download: &Download,
    shell: WeakEntity<Shell>,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let sizes = format!(
        "{} of {}",
        install::size(download.done),
        install::size(download.total)
    );
    div()
        .flex()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(36.))
                .rounded(theme.radius)
                .bg(theme.muted)
                .border_1()
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Box).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .truncate()
                                        .child(download.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Downloading game"),
                                ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("cancel-{id}")))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .tooltip("Cancel")
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    let _ = shell.update(cx, |shell, cx| shell.cancel(&id, cx));
                                }),
                        ),
                )
                .child(
                    Progress::new(SharedString::from(format!("bar-{}", download.name)))
                        .value(download.pct() as f32)
                        .color(theme.primary),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_2()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(sizes)
                        .children(download.left()),
                ),
        )
        .into_any_element()
}
