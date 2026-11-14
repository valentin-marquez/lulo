//! Library: every instance as a square card, grouped by branch, with search and sort.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::base::spring;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::instance::{Instance, Status};
use crate::shell::Shell;
use crate::ui::{self, Empty, FancyButton, SPRING_FAST, SPRING_PRESS};

/// Card width; the art inside is square.
const CARD: f32 = 176.;
const ART: f32 = CARD - 16.;
/// Height of the toolbar's controls: the primary button's middle size.
const CONTROL: Pixels = px(36.);

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    LastPlayed,
    Name,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Sort::LastPlayed => "Last played",
            Sort::Name => "Name",
        }
    }
}

pub struct Library {
    shell: WeakEntity<Shell>,
    search: Entity<InputState>,
    sort: Sort,
    /// Group by branch; off shows one grid.
    // ponytail: groups are the branch; named groups need saving and come with "Create group".
    grouped: bool,
    collapsed: HashSet<SharedString>,
    _search: Subscription,
    /// The search frame shows the focus ring, so focus changes repaint the library.
    _focus: [Subscription; 2],
}

impl Library {
    pub fn new(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let on_search = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        let focus = search.focus_handle(cx);
        let on_focus = [
            cx.on_focus(&focus, window, |_, _, cx| cx.notify()),
            cx.on_blur(&focus, window, |_, _, cx| cx.notify()),
        ];
        Self {
            shell,
            search,
            sort: Sort::LastPlayed,
            grouped: true,
            collapsed: HashSet::new(),
            _search: on_search,
            _focus: on_focus,
        }
    }

    /// The instances to show, filtered and sorted, with each one's status.
    fn shown(&self, cx: &App) -> Vec<(Instance, Status)> {
        let Some(shell) = self.shell.upgrade() else {
            return Vec::new();
        };
        let shell = shell.read(cx);
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut shown: Vec<(Instance, Status)> = shell
            .instances()
            .iter()
            .filter(|i| {
                query.is_empty()
                    || i.name.to_lowercase().contains(&query)
                    || i.branch.to_lowercase().contains(&query)
            })
            .map(|i| (i.clone(), shell.status(i)))
            .collect();
        match self.sort {
            // The store already lists most recently played first.
            Sort::LastPlayed => {}
            Sort::Name => shown.sort_by_key(|(i, _)| i.name.to_lowercase()),
        }
        shown
    }

    /// Search and New instance on one row, sort and grouping under it; every control is
    /// 36 px tall, the primary button's middle size, so the bar reads as one line.
    fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let shell = self.shell.clone();
        let sort = self.sort;
        let grouped = self.grouped;
        let theme = cx.theme();
        let focused = self.search.focus_handle(cx).is_focused(window);
        let search = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                // The input's own frame only comes in 32 or 44 px, so ours carries the
                // border at the toolbar's height and the input sits inside it bare.
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .h(CONTROL)
                    .pl_3()
                    .rounded(theme.radius)
                    .bg(theme.background)
                    .border_1()
                    .border_color(if focused { theme.ring } else { theme.input })
                    .child(
                        Icon::new(IconName::Search)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    )
                    .child(Input::new(&self.search).appearance(false).cleanable(true)),
            )
            .child(
                FancyButton::new("library-new", "New instance")
                    .small()
                    .icon(IconName::Plus)
                    .on_click(move |_, window, cx| Shell::create(shell.clone(), window, cx)),
            );
        let filters = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("library-sort")
                    .outline()
                    .h(CONTROL)
                    .cursor_pointer()
                    .icon(IconName::ArrowDownWideNarrow)
                    .label(sort.label())
                    .dropdown_menu({
                        let this = this.clone();
                        move |mut menu, _, _| {
                            for option in [Sort::LastPlayed, Sort::Name] {
                                let this = this.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(option.label())
                                        .checked(option == sort)
                                        .on_click(move |_, _, cx| {
                                            let _ = this.update(cx, |library, cx| {
                                                library.sort = option;
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            menu
                        }
                    }),
            )
            .child(
                Button::new("library-group")
                    .outline()
                    .h(CONTROL)
                    .cursor_pointer()
                    .icon(IconName::Layers)
                    .label(if grouped {
                        "Group by branch"
                    } else {
                        "No groups"
                    })
                    .dropdown_menu(move |mut menu, _, _| {
                        for (label, value) in [("Group by branch", true), ("No groups", false)] {
                            let this = this.clone();
                            menu = menu.item(
                                PopupMenuItem::new(label)
                                    .checked(value == grouped)
                                    .on_click(move |_, _, cx| {
                                        let _ = this.update(cx, |library, cx| {
                                            library.grouped = value;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            );
        div().flex().flex_col().gap_3().child(search).child(filters)
    }

    fn group(
        &self,
        branch: SharedString,
        cards: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let collapsed = self.collapsed.contains(&branch);
        let count = cards.len();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id(SharedString::from(format!("group-{branch}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.foreground))
                    .child(
                        Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size_4(),
                    )
                    .child(branch.clone())
                    .child(div().text_xs().child(count.to_string()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&branch) {
                            this.collapsed.insert(branch.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(!collapsed, |this| this.child(grid(cards)))
    }
}

fn grid(cards: Vec<AnyElement>) -> Div {
    div().flex().flex_wrap().gap_4().children(cards)
}

/// One instance: square art with its branch and state, name and last played below. Hovering
/// fades in the main action (an icon button) on a spring.
fn card(
    instance: Instance,
    status: Status,
    shell: WeakEntity<Shell>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme().clone();
    let id = instance.id.clone();
    let hovered = window.use_keyed_state(
        SharedString::from(format!("card-hover-{id}")),
        cx,
        |_, _| false,
    );
    let surface: f32 = spring(
        SharedString::from(format!("card-surface-{id}")),
        if *hovered.read(cx) { 1. } else { 0. },
        SPRING_FAST,
        window,
        cx,
    );
    let pressed = window.use_keyed_state(
        SharedString::from(format!("card-pressed-{id}")),
        cx,
        |_, _| false,
    );
    // Pressing shrinks the face to 98% (Modrinth's cards): the layer insets, content stays.
    let press: f32 = spring(
        SharedString::from(format!("card-press-{id}")),
        if *pressed.read(cx) { 1. } else { 0. },
        SPRING_PRESS,
        window,
        cx,
    );
    let (inset_x, inset_y) = (px(CARD * 0.01 * press), px((ART + 72.) * 0.01 * press));
    let (action, icon, enabled) = status.action();
    let menu_action = action.clone();
    // Running and installing keep their button up: it is how you stop them.
    let reveal = match status {
        Status::Running | Status::Installing(_) => 1.,
        _ => surface,
    };

    let badge = |text: SharedString| {
        div()
            .px_1p5()
            .py_0p5()
            .rounded(px(6.))
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .child(text)
    };
    let state = match status {
        Status::Running => Some(
            badge("Running".into())
                .flex()
                .items_center()
                .gap_1()
                .text_color(theme.primary)
                .child(Icon::empty().path("icons/gamepad.svg").size_3()),
        ),
        Status::Outdated => Some(badge("Update".into()).text_color(theme.primary)),
        Status::Installing(pct) => Some(badge(format!("{pct}%").into())),
        _ => None,
    };

    let art = crate::instance::art::art(&instance, ART, theme.radius, cx)
        .relative()
        .child(
            div()
                .absolute()
                .top_2()
                .right_2()
                .child(badge(instance.branch.clone())),
        )
        .children(state.map(|state| div().absolute().top_2().left_2().child(state)))
        .when(reveal > 0.01 && enabled, |this| {
            let (shell, id) = (shell.clone(), id.clone());
            // Grows from its center (28 to 40 px, icon with it) as it fades in: the scale
            // GPUI can't do on a box, done with size inside a fixed slot.
            let size = 28. + 12. * reveal;
            this.child(
                div()
                    .absolute()
                    .bottom_3()
                    .right_3()
                    .size(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(reveal)
                    .child(
                        div()
                            .id(SharedString::from(format!("card-play-{id}")))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(size))
                            .rounded_full()
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.primary_hover))
                            .shadow(vec![BoxShadow {
                                color: hsla(220. / 360., 0.32, 0.08, 0.24),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(2.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                            .tooltip(move |window, cx| {
                                Tooltip::new(action.clone()).build(window, cx)
                            })
                            .child(Icon::new(icon).size(px(size / 2.)))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                Shell::play(shell.clone(), id.clone(), window, cx);
                            }),
                    ),
            )
        });

    div()
        .id(SharedString::from(format!("card-{id}")))
        .relative()
        .flex()
        .flex_col()
        .gap_2()
        .w(px(CARD))
        .p_2()
        .pb_3()
        .cursor_pointer()
        .on_hover(move |is_hovered, window, cx| {
            hovered.update(cx, |h, _| *h = *is_hovered);
            window.refresh();
        })
        .on_mouse_down(MouseButton::Left, {
            let pressed = pressed.clone();
            move |_, window, cx| {
                pressed.update(cx, |p, _| *p = true);
                window.refresh();
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let pressed = pressed.clone();
            move |_, window, cx| {
                pressed.update(cx, |p, _| *p = false);
                window.refresh();
            }
        })
        .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
            pressed.update(cx, |p, _| *p = false);
            window.refresh();
        })
        .child(
            div()
                .absolute()
                .top(inset_y)
                .bottom(inset_y)
                .left(inset_x)
                .right(inset_x)
                .rounded(theme.radius_lg)
                .bg(ui::mix(
                    theme.group_box,
                    ui::hovered(theme.group_box, cx),
                    surface,
                ))
                .border_1()
                .border_color(theme.border),
        )
        .on_click({
            let (shell, id) = (shell.clone(), id.clone());
            move |_, window, cx| Shell::view(shell.clone(), id.clone(), window, cx)
        })
        .child(art)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .px_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(instance.name.clone()),
                )
                // While it downloads, the progress takes the line that says when it was played:
                // on the art it sat under the cancel button, orange on the art's own colors.
                .child(match crate::instance::bar(&id, status, cx) {
                    Some(bar) => div()
                        .h_4()
                        .flex()
                        .items_center()
                        .child(div().w_full().child(bar)),
                    None => div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(instance.played()),
                }),
        )
        // Right click: what the card and the instance page offer, without opening it.
        .context_menu({
            let dir = instance.dir.clone();
            move |menu, _, _| {
                let item = |label: &str, icon: IconName| {
                    PopupMenuItem::new(SharedString::from(label.to_owned())).icon(icon)
                };
                let on = |run: fn(WeakEntity<Shell>, SharedString, &mut Window, &mut App)| {
                    let (shell, id) = (shell.clone(), id.clone());
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        run(shell.clone(), id.clone(), window, cx)
                    }
                };
                let (folder, path) = (dir.clone(), dir.to_string_lossy().into_owned());
                // The main action stands out in the accent, as on the card and the page.
                let (label, accent) = (SharedString::from(menu_action.clone()), theme.primary);
                let play = PopupMenuItem::element(move |_, _| {
                    div()
                        .text_color(accent)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label.clone())
                })
                .icon(Icon::new(icon).text_color(accent));
                menu.item(play.on_click(on(Shell::play)))
                    .separator()
                    .item(item("View instance", IconName::Eye).on_click(on(Shell::view)))
                    .item(
                        item("Open folder", IconName::FolderOpen)
                            .on_click(move |_, _, cx| cx.open_with_system(&folder)),
                    )
                    .item(
                        item("Copy path", IconName::Copy).on_click(move |_, window, cx| {
                            use gpui_kit::component::WindowExt;
                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                            window.push_notification(
                                ui::toast::done("Copied! Paste it anywhere"),
                                cx,
                            );
                        }),
                    )
                    .item(item("Edit icon", IconName::Palette).on_click(on(Shell::customize)))
                    .item(item("Settings", IconName::Settings).on_click(on(Shell::settings)))
                    .separator()
                    .item(item("Delete", IconName::Trash).on_click(on(Shell::confirm_delete)))
            }
        })
        .into_any_element()
}

/// The page's title. With a screenshot of the game it is a banner: the picture, darkened
/// toward the left so the title reads over it; without it (offline, first run) just the title.
fn banner(total: usize, hero: Option<std::path::PathBuf>, cx: &App) -> Div {
    let theme = cx.theme();
    let Some(hero) = hero else {
        return div()
            .text_xl()
            .font_weight(FontWeight::BOLD)
            .child("Library");
    };
    // Just how many: some may not be installed yet, so it claims nothing more.
    let count = match total {
        1 => "1 instance".to_owned(),
        many => format!("{many} instances"),
    };
    div()
        .relative()
        .flex_none()
        .h(px(176.))
        .rounded(theme.radius_lg)
        .overflow_hidden()
        .child(
            img(hero)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(theme.radius_lg)
                .object_fit(ObjectFit::Cover),
        )
        // Darkens the lower part, where the title sits, whatever the picture has there.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(theme.radius_lg)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(black().opacity(0.), 0.2),
                    linear_color_stop(black().opacity(0.7), 1.),
                )),
        )
        .child(
            div()
                .absolute()
                .left_6()
                .bottom_5()
                .flex()
                .flex_col()
                .gap_0p5()
                .text_color(white())
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .child("Library"),
                )
                .child(div().text_sm().child(count)),
        )
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown(cx);
        let total = self
            .shell
            .upgrade()
            .map_or(0, |shell| shell.read(cx).instances().len());
        let handle = self.shell.clone();
        let hero = self
            .shell
            .upgrade()
            .and_then(|shell| shell.read(cx).banner());

        let body = if total == 0 {
            Empty::new(
                IconName::Box,
                "No instances yet",
                "Create one to pick a game version and start playing.",
            )
            .into_any_element()
        } else if shown.is_empty() {
            Empty::new(
                IconName::Search,
                "No instances match",
                "Try another name or branch.",
            )
            .into_any_element()
        } else if self.grouped {
            // Groups keep the order their first instance has in the sort.
            let mut groups: Vec<(SharedString, Vec<AnyElement>)> = Vec::new();
            for (instance, status) in shown {
                let branch = instance.branch.clone();
                let card = card(instance, status, handle.clone(), window, cx);
                match groups.iter_mut().find(|(b, _)| *b == branch) {
                    Some((_, cards)) => cards.push(card),
                    None => groups.push((branch, vec![card])),
                }
            }
            div()
                .flex()
                .flex_col()
                .gap_6()
                .children({
                    let mut sections = Vec::new();
                    for (branch, cards) in groups {
                        sections.push(self.group(branch, cards, cx).into_any_element());
                    }
                    sections
                })
                .into_any_element()
        } else {
            grid(
                shown
                    .into_iter()
                    .map(|(instance, status)| card(instance, status, handle.clone(), window, cx))
                    .collect(),
            )
            .into_any_element()
        };

        div()
            .id("library")
            .flex()
            .flex_col()
            .size_full()
            .gap_4()
            .p_6()
            .overflow_y_scroll()
            .child(banner(total, hero, cx))
            .child(self.toolbar(window, cx))
            .child(body)
            // Right click on the page itself; a card's own menu wins over this one.
            .context_menu(move |menu, _, _| {
                let (create, folder) = (handle.clone(), crate::instance::store::root());
                menu.item(
                    PopupMenuItem::new("New instance")
                        .icon(IconName::Plus)
                        .on_click(move |_, window, cx| Shell::create(create.clone(), window, cx)),
                )
                .item(
                    PopupMenuItem::new("Open instances folder")
                        .icon(IconName::FolderOpen)
                        .on_click(move |_, _, cx| cx.open_with_system(&folder)),
                )
            })
    }
}
