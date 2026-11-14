//! "Add mod" dialog: paste a Workshop link, see what it is, then add it to the instance.

use gpui_kit::assets::IconName;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::install::size;
use super::mods::{self, Listing};
use super::{Instance, branches};
use crate::shell::Shell;
use crate::ui::{self, FancyButton, Loader, Meta};

const WIDTH: f32 = 580.;
/// Side of the mod's Workshop picture.
const PICTURE: f32 = 120.;
/// Tags shown besides the build ones; a mod can carry a dozen.
const TAGS: usize = 4;

pub fn open(instance: Instance, shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| AddMod::new(instance, shell, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        // Fades in settling from just above, and back out (the default drops from the window top).
        let dialog = dialog.entrance(DialogEntrance::FadeSlide);
        dialog
            .title("Add mod")
            .centered()
            .w(px(WIDTH))
            .child(view.clone())
    });
}

/// What is known about the pasted link.
enum Lookup {
    /// Nothing usable typed yet.
    Empty,
    Loading(u64),
    Found(Box<Listing>),
    Failed(String),
}

struct AddMod {
    instance: Instance,
    shell: WeakEntity<Shell>,
    link: Entity<InputState>,
    lookup: Lookup,
    _link: Subscription,
}

/// The build line a mod's Workshop tags say it is for, when they say only one.
fn other_build(tags: &[String], modern: bool) -> Option<&'static str> {
    let has = |tag: &str| tags.iter().any(|t| t == tag);
    match (has("Build 41"), has("Build 42"), modern) {
        (true, false, true) => Some("Build 41"),
        (false, true, false) => Some("Build 42"),
        _ => None,
    }
}

impl AddMod {
    fn new(
        instance: Instance,
        shell: WeakEntity<Shell>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let link = cx.new(|cx| InputState::new(window, cx).placeholder("Workshop link or mod id"));
        let on_link = cx.subscribe(&link, |this, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.typed(cx);
            }
        });
        Self {
            instance,
            shell,
            link,
            lookup: Lookup::Empty,
            _link: on_link,
        }
    }

    /// Looks the link up as soon as it holds an id, so pasting is all it takes.
    fn typed(&mut self, cx: &mut Context<Self>) {
        let typed = self.link.read(cx).value();
        let Some(id) = steam::workshop::parse_id(&typed) else {
            self.lookup = if typed.trim().is_empty() {
                Lookup::Empty
            } else {
                Lookup::Failed("Hmm, that doesn't look like a Workshop link".into())
            };
            return cx.notify();
        };
        let known = match &self.lookup {
            Lookup::Loading(now) => *now == id,
            Lookup::Found(found) => found.item.id == id,
            _ => false,
        };
        if known {
            return;
        }
        self.lookup = Lookup::Loading(id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let found = mods::lookup(id).await;
            let _ = this.update(cx, |this, cx| {
                // Another link was pasted meanwhile: this answer is stale.
                if !matches!(this.lookup, Lookup::Loading(now) if now == id) {
                    return;
                }
                this.lookup = match found {
                    Ok(found) if found.item.manifest == 0 => Lookup::Failed(
                        "That one is a collection. Add those when you create an instance".into(),
                    ),
                    Ok(found) => Lookup::Found(Box::new(found)),
                    Err(why) => Lookup::Failed(why),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Lookup::Found(found) = &self.lookup else {
            return;
        };
        let (instance, id) = (self.instance.clone(), found.item.id);
        let handle = window.window_handle();
        let shell = self.shell.clone();
        window.close_dialog(cx);
        // After this update ends: the shell repaints views as the download starts.
        cx.defer(move |cx| {
            let _ = shell.update(cx, |shell, cx| shell.add_mod(instance, id, handle, cx));
        });
    }

    /// The mod as the Workshop describes it: its picture, then title, facts and tags.
    fn card(&self, found: &Listing, cx: &App) -> Div {
        let theme = cx.theme();
        let item = &found.item;
        let (builds, others): (Vec<&String>, Vec<&String>) =
            item.tags.iter().partition(|tag| tag.starts_with("Build "));
        let warning = other_build(&item.tags, mods::is_modern(&self.instance));
        let chip = |text: &str, strong: bool| {
            div()
                .flex_none()
                .px_2()
                .py_0p5()
                .rounded(px(6.))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .map(|this| {
                    if strong {
                        this.bg(theme.primary.opacity(0.15))
                    } else {
                        this.bg(theme.muted).text_color(theme.muted_foreground)
                    }
                })
                .child(text.to_owned())
        };
        let picture = match &found.picture {
            Some(path) => div().flex_none().size(px(PICTURE)).child(
                img(path.clone())
                    .size_full()
                    .rounded(theme.radius)
                    .object_fit(ObjectFit::Cover),
            ),
            None => div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(PICTURE))
                .rounded(theme.radius)
                .bg(theme.muted)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Puzzle).size_8()),
        };
        div()
            .flex()
            .gap_4()
            .p_3()
            .rounded(theme.radius_lg)
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .child(picture)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(item.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_4()
                            .child(Meta::new(IconName::Download, size(item.size)))
                            .children(item.updated.map(|at| {
                                Meta::new(
                                    IconName::Clock,
                                    format!("Updated {}", branches::date(at as u64)),
                                )
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1p5()
                            .children(builds.iter().map(|tag| chip(tag, true)))
                            .children(others.iter().take(TAGS).map(|tag| chip(tag, false))),
                    )
                    .children(warning.map(|build| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(ui::warning_text(cx))
                            .child(Icon::new(IconName::TriangleAlert).size_4().flex_none())
                            .child(format!(
                                "Heads up! It's for {build} only, not this instance's"
                            ))
                    })),
            )
    }

    /// What fills the card's place before there is a mod to show.
    fn waiting(&self, icon: IconName, text: SharedString, failed: bool, cx: &App) -> Div {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .h(px(PICTURE + 26.))
            .rounded(theme.radius_lg)
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .text_sm()
            .text_color(if failed {
                theme.danger
            } else {
                theme.muted_foreground
            })
            .child(Icon::new(icon).size_6())
            .child(text)
    }
}

impl Render for AddMod {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let focused = self.link.focus_handle(cx).is_focused(window);
        let found = match &self.lookup {
            Lookup::Empty => self
                .waiting(
                    IconName::Puzzle,
                    "Paste a mod's Workshop link and it shows up here".into(),
                    false,
                    cx,
                )
                .into_any_element(),
            Lookup::Loading(_) => self
                .waiting(IconName::Search, "Asking Steam about it".into(), false, cx)
                .child(div().w(px(160.)).child(Loader))
                .into_any_element(),
            Lookup::Failed(why) => self
                .waiting(IconName::TriangleAlert, why.clone().into(), true, cx)
                .into_any_element(),
            Lookup::Found(found) => ui::enter(self.card(found, cx), "mod-found").into_any_element(),
        };
        let ready = matches!(self.lookup, Lookup::Found(_));
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                // The input's own frame only comes in 32 or 44 px, so ours carries the border
                // and the link icon, and the input sits inside it bare.
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.))
                    .pl_3()
                    .rounded(theme.radius)
                    .bg(theme.background)
                    .border_1()
                    .border_color(if focused { theme.ring } else { theme.input })
                    .child(
                        Icon::new(IconName::Link)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    )
                    .child(Input::new(&self.link).appearance(false).cleanable(true)),
            )
            // The card's place has one height in every state, so the dialog never jumps.
            .child(found)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        ui::secondary("mod-add-cancel", "Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .when(ready, |this| {
                        this.child(
                            FancyButton::new("mod-add-confirm", "Add mod")
                                .small()
                                .icon(IconName::Plus)
                                .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::other_build;

    #[test]
    fn warns_only_when_the_tags_name_the_other_build_alone() {
        let tags = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert_eq!(other_build(&tags(&["Build 41"]), true), Some("Build 41"));
        assert_eq!(
            other_build(&tags(&["Build 42", "QOL"]), false),
            Some("Build 42")
        );
        assert_eq!(other_build(&tags(&["Build 41", "Build 42"]), true), None);
        assert_eq!(other_build(&tags(&["Build 42"]), true), None);
        assert_eq!(
            other_build(&tags(&["QOL"]), true),
            None,
            "no build tag says nothing"
        );
    }
}
