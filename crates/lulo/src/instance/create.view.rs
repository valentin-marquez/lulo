//! "Create instance" dialog: pick where the instance comes from and which game build it runs.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::path::PathBuf;
use std::rc::Rc;

use super::branches::{self, Branch};
use super::{Instance, store};
use crate::ui::{FancyButton, Loader};

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Custom,
    Collection,
    Import,
}

impl Source {
    const ALL: [Source; 3] = [Source::Custom, Source::Collection, Source::Import];

    fn label(self) -> &'static str {
        match self {
            Source::Custom => "Custom",
            Source::Collection => "Collection",
            Source::Import => "Import",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Source::Custom => IconName::Box,
            Source::Collection => IconName::Compass,
            Source::Import => IconName::FolderOpen,
        }
    }
}

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
        dialog
            .title("Create instance")
            .w(px(760.))
            .child(view.clone())
    });
}

struct Create {
    root: PathBuf,
    created: Created,
    source: Source,
    /// `None` while loading from Steam.
    branches: Option<Result<Vec<Branch>, String>>,
    branch: usize,
    name: Entity<InputState>,
    link: Entity<InputState>,
    path: Entity<InputState>,
}

impl Create {
    fn new(root: PathBuf, created: Created, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            root,
            created,
            source: Source::Custom,
            branches: None,
            branch: 0,
            name: cx.new(|cx| InputState::new(window, cx).placeholder("New instance")),
            link: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://steamcommunity.com/sharedfiles/filedetails/?id=")
            }),
            path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Folder with a Project Zomboid install")
            }),
        }
    }

    fn sources(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w(px(160.))
            .flex_none()
            .p_1()
            .bg(theme.muted)
            .children(Source::ALL.map(|source| {
                let active = source == self.source;
                div()
                    .id(source.label())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded(theme.radius)
                    .text_sm()
                    .cursor_pointer()
                    .map(|this| {
                        if active {
                            this.bg(theme.group_box)
                                .border_1()
                                .border_color(theme.border)
                                .font_weight(FontWeight::SEMIBOLD)
                        } else {
                            this.text_color(theme.muted_foreground)
                                .hover(|s| s.text_color(theme.foreground))
                        }
                    })
                    .child(Icon::new(source.icon()).size_4())
                    .child(source.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.source = source;
                        cx.notify();
                    }))
            }))
    }

    fn versions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let cell = |w: Pixels| div().w(w).flex_none().truncate();
        let header = div()
            .flex()
            .px_3()
            .py_2()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.muted_foreground)
            .border_b_1()
            .border_color(theme.border)
            .child(cell(px(130.)).child("Branch"))
            .child(div().flex_1().child("Description"))
            .child(cell(px(96.)).child("Build"))
            .child(cell(px(96.)).child("Updated"));

        let list = match &self.branches {
            None => {
                return div().flex().flex_col().child(header).child(Loader).child(
                    div()
                        .px_3()
                        .py_3()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Loading versions from Steam"),
                );
            }
            Some(Err(err)) => {
                return div().flex().flex_col().child(header).child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_3()
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
                        ),
                );
            }
            Some(Ok(list)) => list,
        };
        let rows = list.iter().enumerate().map(|(ix, branch)| {
            let selected = ix == self.branch;
            div()
                .id(ix)
                .flex()
                .items_center()
                .px_3()
                .py_2()
                .text_sm()
                .cursor_pointer()
                .map(|this| {
                    if selected {
                        this.bg(theme.primary.opacity(0.08))
                    } else {
                        this.hover(|s| s.bg(theme.muted))
                    }
                })
                .child(
                    cell(px(130.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            div()
                                .size(px(14.))
                                .flex_none()
                                .rounded_full()
                                .border_1()
                                .border_color(if selected {
                                    theme.primary
                                } else {
                                    theme.border
                                })
                                .when(selected, |this| this.border(px(4.))),
                        )
                        .child(branch.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(match (&branch.description, branch.name.as_str()) {
                            (Some(text), _) => text.clone(),
                            (None, "public") => "Current release".into(),
                            (None, _) => String::new(),
                        }),
                )
                .child(
                    cell(px(96.))
                        .text_color(theme.muted_foreground)
                        .child(branch.build_id.to_string()),
                )
                .child(
                    cell(px(96.))
                        .text_color(theme.muted_foreground)
                        .child(branch.updated.map(branches::date).unwrap_or_default()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.branch = ix;
                    cx.notify();
                }))
        });

        div().flex().flex_col().child(header).children(rows)
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
            "New instance".to_owned()
        } else {
            typed
        };
        let Some(Ok(list)) = &self.branches else {
            return;
        };
        let branch = list[self.branch].name.clone();
        match store::create(&self.root, &name, &branch) {
            Ok(instance) => {
                window.close_dialog(cx);
                (self.created)(instance, window, cx);
            }
            Err(err) => {
                window.push_notification(format!("Couldn't create the instance: {err}"), cx)
            }
        }
    }

    fn field(label: &'static str, input: &Entity<InputState>, hint: &'static str, cx: &App) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .child(Input::new(input))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            )
    }
}

impl Render for Create {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = match self.source {
            Source::Custom => div().child(self.versions(cx)),
            Source::Collection => div()
                .child(Self::field(
                    "Steam collection link",
                    &self.link,
                    "Its mods are installed into the new instance.",
                    cx,
                ))
                .child(self.versions(cx)),
            Source::Import => div().child(
                Self::field(
                    "Install folder",
                    &self.path,
                    "Copies the game, mods and saves into a new instance.",
                    cx,
                )
                .child(
                    div().child(
                        Button::new("browse")
                            .cursor_pointer()
                            .outline()
                            .small()
                            .icon(IconName::FolderOpen)
                            .label("Browse"),
                    ),
                ),
            ),
        };
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(56.))
                            .rounded(theme.radius_lg)
                            .bg(theme.muted)
                            .border_1()
                            .border_color(theme.border)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::Box).size_6()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Name"),
                            )
                            .child(Input::new(&self.name)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .min_h(px(260.))
                    .rounded(theme.radius_lg)
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .child(self.sources(cx))
                    .child(div().flex_1().min_w_0().child(panel)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .outline()
                            .cursor_pointer()
                            .label("Cancel")
                            // Same height and radius as the small FancyButton beside it.
                            .h(px(36.))
                            .px_3()
                            .rounded(px(8.))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    // ponytail: only Custom creates for now; collections and imports come later.
                    .when(
                        self.source == Source::Custom && matches!(self.branches, Some(Ok(_))),
                        |this| {
                            this.child(
                                FancyButton::new("create", "Create instance")
                                    .small()
                                    .icon(IconName::Plus)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.create(window, cx)),
                                    ),
                            )
                        },
                    ),
            )
    }
}
