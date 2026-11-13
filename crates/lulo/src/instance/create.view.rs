//! "Create instance" dialog, in two steps: pick how to start, then set the picture, name and
//! game version.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon as Glyph, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::path::PathBuf;
use std::rc::Rc;

use super::branches::{self, Branch};
use super::picture::{self, Picker};
use super::{Instance, store};
use crate::ui::{FancyButton, Loader};

/// The step the dialog is on.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Choose,
    Custom,
    Collection,
    Import,
}

/// Height shared by the inputs and buttons of the form.
const CONTROL: Pixels = px(36.);

type Created = Rc<dyn Fn(Instance, &mut Window, &mut App)>;

/// Opens the dialog. `created` runs after a new instance is written under `root`.
pub fn open(
    root: PathBuf,
    created: impl Fn(Instance, &mut Window, &mut App) + 'static,
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
    /// The picture so far; Customize lives in the instance's settings.
    picture: Entity<Picker>,
    name: Entity<InputState>,
    /// The name used when the field is left empty; it shows as the placeholder.
    suggested: &'static str,
    link: Entity<InputState>,
    path: Entity<InputState>,
}

impl Create {
    fn new(root: PathBuf, created: Created, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let suggested = super::suggest();
        Self {
            root,
            created,
            step: Step::Choose,
            forward: true,
            branches: None,
            branch: 0,
            picture: cx.new(|_| Picker::new(None, false)),
            name: cx.new(|cx| InputState::new(window, cx).placeholder(suggested)),
            suggested,
            link: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://steamcommunity.com/sharedfiles/filedetails/?id=")
            }),
            path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Folder with a Project Zomboid install")
            }),
        }
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
        let name = if typed.is_empty() {
            self.suggested.to_owned()
        } else {
            typed
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
                (self.created)(instance, window, cx);
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
            (
                Step::Import,
                IconName::FolderOpen,
                "Import an existing install",
                "Bring over a Steam or KnoxKit install with its mods and saves.",
                true,
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
                        Button::new("retry")
                            .outline()
                            .cursor_pointer()
                            .label("Retry")
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

    /// Back on the left, the step's action on the right.
    fn footer(&self, action: Option<AnyElement>, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .pt_2()
            .child(
                Button::new("back")
                    .outline()
                    .cursor_pointer()
                    .icon(IconName::ChevronLeft)
                    .label("Back")
                    .h(CONTROL)
                    .px_3()
                    .rounded(px(8.))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Step::Choose, cx))),
            )
            .children(action)
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
                let ready = matches!(self.branches, Some(Ok(_)));
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(self.picture.clone())
                    .when(self.step == Step::Collection, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1p5()
                                .child(label("Collection link"))
                                .child(Input::new(&self.link)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(label("Name"))
                            .child(Input::new(&self.name)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(label("Game version"))
                            .child(self.versions(cx)),
                    )
                    // ponytail: collections create a plain instance until #20 installs their mods.
                    .child({
                        let create = ready.then(|| {
                            FancyButton::new("create", "Create instance")
                                .small()
                                .icon(IconName::Plus)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.create(window, cx)),
                                )
                                .into_any_element()
                        });
                        self.footer(create, cx)
                    })
            }
            Step::Import => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(label("Install folder"))
                        .child(Input::new(&self.path))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Copies the game, mods and saves into a new instance."),
                        ),
                )
                // ponytail: importing lands with #23; the step only collects the folder for now.
                .child(self.footer(None, cx)),
        };
        body.with_animation(
            step,
            Animation::new(crate::ui::ENTER).with_easing(crate::ui::ease_out_quint),
            move |body, t| body.relative().left(px(from * (1. - t))).opacity(t),
        )
    }
}
