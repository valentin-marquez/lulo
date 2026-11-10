//! "Create instance" dialog: pick where the instance comes from and which game build it runs.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Branch, branches};
use crate::ui::FancyButton;

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

pub fn open(window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| Create::new(window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title("Create instance")
            .w(px(760.))
            .child(view.clone())
    });
}

struct Create {
    source: Source,
    branches: Vec<Branch>,
    branch: usize,
    name: Entity<InputState>,
    link: Entity<InputState>,
    path: Entity<InputState>,
}

impl Create {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            source: Source::Custom,
            branches: branches(),
            branch: 0,
            name: cx.new(|cx| InputState::new(window, cx).placeholder("New instance")),
            link: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://steamcommunity.com/sharedfiles/filedetails/?id=…")
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

        let rows = self.branches.iter().enumerate().map(|(ix, branch)| {
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
                        .child(branch.name),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(branch.description),
                )
                .child(
                    cell(px(96.))
                        .text_color(theme.muted_foreground)
                        .child(branch.build),
                )
                .child(
                    cell(px(96.))
                        .text_color(theme.muted_foreground)
                        .child(branch.updated),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.branch = ix;
                    cx.notify();
                }))
        });

        div().flex().flex_col().child(header).children(rows)
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
                            .outline()
                            .small()
                            .icon(IconName::FolderOpen)
                            .label("Browse…"),
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
                            .label("Cancel")
                            // Same height and radius as the small FancyButton beside it.
                            .h(px(36.))
                            .px_3()
                            .rounded(px(8.))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    // ponytail: closes without creating anything until instances persist to disk.
                    .child(
                        FancyButton::new("create", "Create instance")
                            .small()
                            .icon(IconName::Plus)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
    }
}
