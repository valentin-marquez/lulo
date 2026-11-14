//! "Create instance" dialog, in two steps: pick how to start, then set the picture, name and
//! game version.

use gpui_kit::assets::IconName;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon as Glyph, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::path::PathBuf;
use std::rc::Rc;

use super::branches::{self, Branch};
use super::install::size;
use super::mods::{self, Bundle};
use super::picture::{self, Picker};
use super::{Instance, store};
use crate::ui::{self, FancyButton, Loader, Meta};

/// The step the dialog is on.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Choose,
    Custom,
    Collection,
}

/// A collection's name and the Workshop items to install from it.
pub type Mods = Option<(SharedString, Vec<u64>)>;

type Created = Rc<dyn Fn(Instance, Mods, &mut Window, &mut App)>;

/// What is known about the pasted collection link.
enum Lookup {
    /// Nothing usable typed yet.
    Empty,
    Loading(u64),
    Found(Box<Bundle>),
    Failed(String),
}

/// Opens the dialog. `created` runs after a new instance is written under `root`, with the
/// mods to install into it when it started from a collection.
pub fn open(
    root: PathBuf,
    created: impl Fn(Instance, Mods, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let created: Created = Rc::new(created);
    let view = cx.new(|cx| {
        let mut create = Create::new(root, created, window, cx);
        create.load(cx);
        create
    });
    window.open_dialog(cx, move |dialog, _, _| {
        // Fades in settling from just above, and back out (the default drops from the window top).
        let dialog = dialog.entrance(DialogEntrance::FadeSlide);
        dialog
            .title("Create instance")
            .centered()
            .w(px(620.))
            .child(view.clone())
    });
}

struct Create {
    root: PathBuf,
    created: Created,
    step: Step,
    /// Whether the last step change went forward; steps slide in from that side.
    forward: bool,
    /// `None` while loading from Steam.
    branches: Option<Result<Vec<Branch>, String>>,
    branch: usize,
    /// The picture so far, beside the name field.
    picture: Entity<Picker>,
    name: Entity<InputState>,
    /// The name used when the field is left empty; it shows as the placeholder.
    suggested: &'static str,
    link: Entity<InputState>,
    collection: Lookup,
    _link: Subscription,
}

impl Create {
    fn new(root: PathBuf, created: Created, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let suggested = super::suggest();
        let link = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://steamcommunity.com/sharedfiles/filedetails/?id=")
        });
        let on_link = cx.subscribe_in(&link, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::Change = event {
                this.typed(window, cx);
            }
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(suggested));
        Self {
            root,
            created,
            step: Step::Choose,
            forward: true,
            branches: None,
            branch: 0,
            picture: cx.new(|_| Picker::new(None, false).with_name(name.clone())),
            name,
            suggested,
            link,
            collection: Lookup::Empty,
            _link: on_link,
        }
    }

    /// Looks the collection up as soon as the link holds an id, so pasting is all it takes.
    fn typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.link.read(cx).value();
        let Some(id) = steam::workshop::parse_id(&typed) else {
            self.collection = if typed.trim().is_empty() {
                Lookup::Empty
            } else {
                Lookup::Failed("Hmm, that doesn't look like a Workshop link".into())
            };
            return cx.notify();
        };
        let known = match &self.collection {
            Lookup::Loading(now) => *now == id,
            Lookup::Found(found) => found.id == id,
            _ => false,
        };
        if known {
            return;
        }
        self.collection = Lookup::Loading(id);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let found = mods::collection(id).await;
            let _ = this.update_in(cx, |this, window, cx| {
                // Another link was pasted meanwhile: this answer is stale.
                if !matches!(this.collection, Lookup::Loading(now) if now == id) {
                    return;
                }
                this.collection = match found {
                    Ok(found) => {
                        this.adopt(&found, window, cx);
                        Lookup::Found(Box::new(found))
                    }
                    Err(why) => Lookup::Failed(why),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The instance takes the collection's name (as the one used when the field is left
    /// empty) and its picture; both can still be changed.
    fn adopt(&mut self, found: &Bundle, window: &mut Window, cx: &mut Context<Self>) {
        let title = found.title.clone();
        self.name
            .update(cx, |name, cx| name.set_placeholder(title, window, cx));
        if let Some(file) = &found.picture {
            self.picture.update(cx, |picker, cx| {
                picker.icon = super::Icon::Image {
                    file: file.to_string_lossy().into_owned(),
                };
                cx.notify();
            });
        }
    }

    /// Whether the chosen game version is Build 42 or newer.
    fn modern(&self) -> bool {
        match &self.branches {
            Some(Ok(list)) => list
                .get(self.branch)
                .is_none_or(|b| mods::branch_is_modern(&b.name)),
            _ => true,
        }
    }

    /// The collection's name and the items that fit the chosen version, once it is found
    /// and has some.
    fn mods(&self) -> Mods {
        let Lookup::Found(found) = &self.collection else {
            return None;
        };
        let (fit, _) = found.split(self.modern());
        (!fit.is_empty()).then(|| {
            (
                found.title.clone().into(),
                fit.iter().map(|item| item.id).collect(),
            )
        })
    }

    fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        self.forward = step != Step::Choose;
        self.step = step;
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.branches = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = branches::fetch().await;
            let _ = this.update(cx, |this, cx| {
                this.branch = 0;
                this.branches = Some(result);
                cx.notify();
            });
        })
        .detach();
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.name.read(cx).value().trim().to_owned();
        let mods = self.mods();
        if self.step == Step::Collection && mods.is_none() {
            return;
        }
        let name = match (typed.is_empty(), &mods) {
            (false, _) => typed,
            (true, Some((collection, _))) => collection.to_string(),
            (true, None) => self.suggested.to_owned(),
        };
        let Some(Ok(list)) = &self.branches else {
            return;
        };
        let branch = list[self.branch].name.clone();
        let made = store::create(&self.root, &name, &branch).and_then(|mut instance| {
            let icon = picture::keep(&self.picture.read(cx).icon, &instance.dir)?;
            store::set_icon(&instance.dir, Some(icon.clone()))?;
            instance.icon = Some(icon);
            Ok(instance)
        });
        match made {
            Ok(instance) => {
                window.close_dialog(cx);
                (self.created)(instance, mods, window, cx);
            }
            Err(err) => window.push_notification(
                crate::ui::toast::failed_because("Well, that instance didn't happen", err),
                cx,
            ),
        }
    }

    /// Step 1: one row per way to start.
    fn choose(&self, cx: &mut Context<Self>) -> Div {
        let options = [
            (
                Step::Custom,
                IconName::Box,
                "Custom setup",
                "Start from scratch with a game version you pick.",
                true,
            ),
            (
                Step::Collection,
                IconName::Compass,
                "From a Workshop collection",
                "Paste a collection link and get its mods installed.",
                true,
            ),
            (
                Step::Choose,
                IconName::Download,
                "Import a .lulopack",
                "Open a shared pack with its build, mods and settings. Coming soon.",
                false,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(options.into_iter().enumerate().map(
                |(ix, (step, icon, title, line, enabled))| {
                    option(ix, icon, title, line, enabled, cx).when(enabled, |this| {
                        this.on_click(cx.listener(move |this, _, _, cx| this.go(step, cx)))
                    })
                },
            ))
    }

    /// Game version: one chip per Steam branch, the chosen one filled.
    fn versions(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let list = match &self.branches {
            None => {
                return div().flex().flex_col().gap_2().child(Loader).child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Loading versions from Steam"),
                );
            }
            Some(Err(err)) => {
                return div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .text_color(theme.muted_foreground)
                            .child(format!("Couldn't reach Steam: {err}")),
                    )
                    .child(
                        ui::secondary("retry", "Retry")
                            .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                    );
            }
            Some(Ok(list)) => list,
        };
        let chips = list.iter().enumerate().map(|(ix, branch)| {
            let selected = ix == self.branch;
            div()
                .id(ix)
                .flex()
                .items_center()
                .gap_1p5()
                .h(px(34.))
                .px_3()
                .rounded(theme.radius)
                .border_1()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .map(|this| {
                    if selected {
                        this.bg(theme.primary.opacity(0.25))
                            .border_color(theme.primary)
                    } else {
                        this.bg(theme.background)
                            .border_color(theme.border)
                            .hover(|s| s.bg(crate::ui::hovered(theme.background, cx)))
                    }
                })
                .when(selected, |this| {
                    this.child(Glyph::new(IconName::Check).size_4())
                })
                .child(branch.name.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.branch = ix;
                    cx.notify();
                }))
        });
        let chosen = &list[self.branch.min(list.len().saturating_sub(1))];
        let about = match (&chosen.description, chosen.name.as_str()) {
            (Some(text), _) => text.clone(),
            (None, "public") => "Current release".into(),
            (None, _) => String::new(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().flex().flex_wrap().gap_2().children(chips))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .when(!about.is_empty(), |this| this.child(about))
                    .child(format!("Build {}", chosen.build_id))
                    .children(
                        chosen
                            .updated
                            .map(|at| format!("Updated {}", branches::date(at))),
                    ),
            )
    }

    /// What the pasted collection holds for the chosen version, under its link. Every state
    /// takes the same height, so the form below never jumps.
    fn summary(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let frame = div()
            .flex()
            .flex_col()
            .justify_center()
            .gap_2()
            .h(SUMMARY)
            .px_4()
            .rounded(theme.radius_lg)
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border);
        let note = |icon: IconName, text: SharedString, color: Hsla| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .text_sm()
                .text_color(color)
                .child(Glyph::new(icon).size_4().flex_none())
                .child(div().min_w_0().truncate().child(text))
        };
        match &self.collection {
            Lookup::Empty => frame
                .items_center()
                .child(note(
                    IconName::Link,
                    "Paste a collection link and what's in it shows up here".into(),
                    theme.muted_foreground,
                ))
                .into_any_element(),
            Lookup::Loading(_) => frame
                .items_center()
                .child(note(
                    IconName::Search,
                    "Asking Steam what's in it".into(),
                    theme.muted_foreground,
                ))
                .child(div().w(px(160.)).child(Loader))
                .into_any_element(),
            Lookup::Failed(why) => frame
                .items_center()
                .child(note(
                    IconName::TriangleAlert,
                    why.clone().into(),
                    theme.danger,
                ))
                .into_any_element(),
            Lookup::Found(found) => {
                let modern = self.modern();
                let (fit, other) = found.split(modern);
                let total: u64 = fit.iter().map(|item| item.size).sum();
                let build = if modern { "Build 41" } else { "Build 42" };
                let facts = div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .min_w_0()
                    .map(|this| {
                        if fit.is_empty() {
                            this.child(note(
                                IconName::TriangleAlert,
                                "Nothing in it runs on this version".into(),
                                theme.danger,
                            ))
                        } else {
                            this.child(Meta::new(
                                IconName::Puzzle,
                                match fit.len() {
                                    1 => "1 mod".to_owned(),
                                    n => format!("{n} mods"),
                                },
                            ))
                            .child(Meta::new(IconName::Download, size(total)))
                        }
                    })
                    .when(!other.is_empty(), |this| {
                        this.child(left_out(&other, build, cx))
                    });
                let card = frame
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(found.title.clone()),
                    )
                    .child(facts);
                ui::enter(card, "collection-found").into_any_element()
            }
        }
    }

    /// Back on the left and Create on the right, always there; Create waits, dimmed, until
    /// there is something to create.
    fn footer(&self, ready: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .justify_between()
            .child(
                ui::secondary("back", "Back")
                    .icon(IconName::ChevronLeft)
                    .on_click(cx.listener(|this, _, _, cx| this.go(Step::Choose, cx))),
            )
            .child(
                FancyButton::new("create", "Create instance")
                    .small()
                    .icon(IconName::Plus)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
            )
    }
}

fn option(
    ix: usize,
    icon: IconName,
    title: &'static str,
    line: &'static str,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(ix)
        .flex()
        .items_center()
        .gap_3()
        .p_4()
        .rounded(theme.radius_lg)
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
        .map(|this| {
            if enabled {
                this.cursor_pointer()
                    .hover(|s| s.bg(crate::ui::hovered(theme.group_box, cx)))
            } else {
                this.opacity(0.5)
            }
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(44.))
                .rounded(theme.radius)
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .child(Glyph::new(icon).size_5()),
        )
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
                        .text_color(theme.muted_foreground)
                        .child(line),
                ),
        )
}

/// Height of the collection summary in every state: its title and one line of facts.
const SUMMARY: Pixels = px(80.);

/// How many of the collection's mods stay out for being made for `build` alone, underlined;
/// hovering lists them, each one opening its Workshop page.
fn left_out(other: &[&steam::workshop::Item], build: &'static str, cx: &App) -> HoverCard {
    let color = ui::warning_text(cx);
    let items: Rc<Vec<(u64, String)>> = Rc::new(
        other
            .iter()
            .map(|item| (item.id, item.title.clone()))
            .collect(),
    );
    let count = items.len();
    HoverCard::new("left-out")
        .open_delay(std::time::Duration::from_millis(300))
        .trigger(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1p5()
                .text_sm()
                .text_color(color)
                .cursor_pointer()
                .child(Glyph::new(IconName::TriangleAlert).size_4())
                .child(div().underline().child(format!("{count} left out"))),
        )
        .content(move |_, _, cx| {
            let theme = cx.theme();
            let rows = items.iter().enumerate().map(|(ix, (id, title))| {
                let url = format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}");
                div()
                    .id(("left-out", ix))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded(theme.radius)
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.muted))
                    .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                    .child(
                        Glyph::new(IconName::ExternalLink)
                            .size_3p5()
                            .text_color(theme.muted_foreground),
                    )
                    .on_click(move |_, _, cx| cx.open_url(&url))
            });
            div()
                .flex()
                .flex_col()
                .gap_1()
                .w(px(300.))
                .child(
                    div()
                        .px_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("For {build} only, so they stay out")),
                )
                .child(
                    div()
                        .id("left-out-list")
                        .flex()
                        .flex_col()
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .children(rows),
                )
        })
}

fn label(text: &'static str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

impl Render for Create {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Page side-by-side: each step slides 8 px in from the side it was reached from.
        let from = if self.forward { 8. } else { -8. };
        let step = ElementId::Name(format!("step-{}", self.step as u8).into());
        let body = match self.step {
            Step::Choose => self.choose(cx),
            Step::Custom | Step::Collection => {
                let ready = matches!(self.branches, Some(Ok(_)))
                    && (self.step != Step::Collection || self.mods().is_some());
                let versions = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(label("Game version"))
                    .child(self.versions(cx));
                // From a collection the link comes first: it fills in the picture and the
                // name, and the version sits under its summary, whose warning depends on it.
                let body = div().flex().flex_col().gap_6();
                match self.step {
                    Step::Collection => body
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(label("Collection link"))
                                .child(Input::new(&self.link))
                                .child(self.summary(cx)),
                        )
                        .child(versions)
                        .child(self.picture.clone()),
                    _ => body.child(self.picture.clone()).child(versions),
                }
                .child(self.footer(ready, cx))
            }
        };
        body.with_animation(
            step,
            Animation::new(crate::ui::ENTER).with_easing(crate::ui::ease_out_quint),
            move |body, t| body.relative().left(px(from * (1. - t))).opacity(t),
        )
    }
}
