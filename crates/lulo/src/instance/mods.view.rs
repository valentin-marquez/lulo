//! The Mods tab: the instance's mods in a table (mod, version, actions) with search, sort and a
//! filter above, and a bar of actions for the rows that are checked (Modrinth App's content
//! tab).

use std::collections::HashSet;
use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Instance;
use super::mods::{self, Mod};
use crate::shell::Shell;
use crate::ui::{self, Empty, FancyButton, Loader, toast};

/// Side of a mod's picture in its row.
const ICON: Pixels = px(40.);
/// Height of the toolbar's controls: the primary button's small size.
const CONTROL: Pixels = px(36.);
/// Width of the Version and Actions columns; Mod takes the rest.
const VERSION: Pixels = px(190.);
const ACTIONS: Pixels = px(132.);

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    OnFirst,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Sort::Name => "Name",
            Sort::OnFirst => "On first",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    On,
    Off,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "All mods",
            Filter::On => "On",
            Filter::Off => "Off",
        }
    }

    fn keeps(self, item: &Mod) -> bool {
        match self {
            Filter::All => true,
            Filter::On => item.enabled,
            Filter::Off => !item.enabled,
        }
    }
}

pub struct Mods {
    instance: Instance,
    /// Read from disk when the tab opens and after every change.
    mods: Vec<Mod>,
    /// What [`mods::stamp`] said when `mods` was read; the same stamp means nothing new.
    stamp: mods::Stamp,
    /// A read is running in the background; `stale` asks for one more after it.
    loading: bool,
    stale: bool,
    search: Entity<InputState>,
    sort: Sort,
    filter: Filter,
    /// Folders of the checked rows.
    selected: HashSet<String>,
    shell: WeakEntity<Shell>,
    /// The table's scroll position, eased by `ui::smooth_scroll`.
    scroll: UniformListScrollHandle,
    _search: Subscription,
}

impl Mods {
    pub fn new(
        instance: Instance,
        shell: WeakEntity<Shell>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search mods"));
        let on_search = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        Self {
            stamp: mods::stamp(&instance),
            loading: false,
            stale: false,
            mods: mods::list(&instance),
            instance,
            search,
            sort: Sort::Name,
            filter: Filter::All,
            selected: HashSet::new(),
            shell,
            scroll: UniformListScrollHandle::new(),
            _search: on_search,
        }
    }

    /// Reads the mods from disk again when they changed; the shell calls it on every step of
    /// a download. The read goes to the background (a collection is hundreds of `mod.info`
    /// files) and only one runs at a time.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let stamp = mods::stamp(&self.instance);
        if stamp == self.stamp {
            return;
        }
        if self.loading {
            self.stale = true;
            return;
        }
        self.loading = true;
        let instance = self.instance.clone();
        cx.spawn(async move |this, cx| {
            let found = crate::runtime::blocking(move || mods::list(&instance)).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(found) = found {
                    this.mods = found;
                    this.stamp = stamp;
                    // A mod that is gone can't stay checked.
                    this.selected
                        .retain(|folder| this.mods.iter().any(|m| m.folder == *folder));
                    cx.notify();
                }
                if std::mem::take(&mut this.stale) {
                    this.refresh(cx);
                }
            });
        })
        .detach();
    }

    fn shown(&self, cx: &App) -> Vec<&Mod> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut shown: Vec<&Mod> = self
            .mods
            .iter()
            .filter(|item| self.filter.keeps(item))
            .filter(|item| {
                query.is_empty()
                    || [&item.name, &item.id, &item.author]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&query))
            })
            .collect();
        // `mods` is already by name; the sort is stable, so names stay in order within each half.
        if self.sort == Sort::OnFirst {
            shown.sort_by_key(|item| !item.enabled);
        }
        shown
    }

    fn toggle(&mut self, id: &str, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        match mods::set_enabled(&self.instance, id, on) {
            Ok(missing) if !missing.is_empty() => window.push_notification(
                toast::heads_up("It needs mods you don't have yet", missing.join(", ")),
                cx,
            ),
            Ok(_) => {}
            Err(err) => window.push_notification(
                toast::failed_because("Hmm, that switch didn't stick", err),
                cx,
            ),
        }
        self.refresh(cx);
    }

    fn remove(&mut self, folder: &str, window: &mut Window, cx: &mut Context<Self>) {
        match mods::remove(&self.instance, folder) {
            Ok(()) => window.push_notification(toast::done("Poof! That mod is gone"), cx),
            Err(err) => {
                window.push_notification(toast::failed_because("That mod wouldn't budge", err), cx)
            }
        }
        self.refresh(cx);
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let this = cx.entity().downgrade();
        let (sort, filter) = (self.sort, self.filter);
        let (instance, shell) = (self.instance.clone(), self.shell.clone());
        let pick = |id: &'static str, icon: IconName, label: &'static str| {
            // The Library's sort and group buttons, so both toolbars read the same.
            Button::new(id)
                .outline()
                .h(CONTROL)
                .cursor_pointer()
                .icon(icon)
                .label(label)
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                // The input's own frame only comes in 32 or 44 px, so ours carries the border
                // at the toolbar's height and the input sits inside it bare.
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
                    .border_color(theme.input)
                    .child(
                        Icon::new(IconName::Search)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    )
                    .child(Input::new(&self.search).appearance(false).cleanable(true)),
            )
            .child(
                pick("mods-sort", IconName::ArrowDownWideNarrow, sort.label()).dropdown_menu({
                    let this = this.clone();
                    move |mut menu, _, _| {
                        for option in [Sort::Name, Sort::OnFirst] {
                            let this = this.clone();
                            menu = menu.item(
                                PopupMenuItem::new(option.label())
                                    .checked(option == sort)
                                    .on_click(move |_, _, cx| {
                                        let _ = this.update(cx, |mods, cx| {
                                            mods.sort = option;
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
                pick("mods-filter", IconName::Layers, filter.label()).dropdown_menu(
                    move |mut menu, _, _| {
                        for option in [Filter::All, Filter::On, Filter::Off] {
                            let this = this.clone();
                            menu = menu.item(
                                PopupMenuItem::new(option.label())
                                    .checked(option == filter)
                                    .on_click(move |_, _, cx| {
                                        let _ = this.update(cx, |mods, cx| {
                                            mods.filter = option;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    },
                ),
            )
            .child(
                FancyButton::new("mod-add", "Add mod")
                    .small()
                    .icon(IconName::Plus)
                    .on_click(move |_, window, cx| {
                        super::mod_add::open(instance.clone(), shell.clone(), window, cx)
                    }),
            )
    }

    /// The checked mods, in the list's order.
    fn chosen(&self) -> Vec<&Mod> {
        self.mods
            .iter()
            .filter(|m| self.selected.contains(&m.folder))
            .collect()
    }

    fn toggle_chosen(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.chosen().iter().map(|m| m.id.clone()).collect();
        match mods::set_enabled_all(&self.instance, &ids, on) {
            Ok(missing) if !missing.is_empty() => window.push_notification(
                toast::heads_up("Some need mods you don't have yet", missing.join(", ")),
                cx,
            ),
            Ok(_) => {}
            Err(err) => window.push_notification(
                toast::failed_because("Hmm, those switches didn't stick", err),
                cx,
            ),
        }
        self.refresh(cx);
    }

    fn remove_chosen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folders: Vec<String> = self.chosen().iter().map(|m| m.folder.clone()).collect();
        let failed = folders
            .iter()
            .find_map(|folder| mods::remove(&self.instance, folder).err());
        match failed {
            None => window.push_notification(
                toast::done(match folders.len() {
                    1 => "Poof! That mod is gone".to_owned(),
                    many => format!("Poof! {many} mods are gone"),
                }),
                cx,
            ),
            Some(err) => {
                window.push_notification(toast::failed_because("A mod wouldn't budge", err), cx)
            }
        }
        self.refresh(cx);
    }

    /// The table's first row: check all, and the column names.
    fn header(&self, shown: &[Mod], cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let all = !shown.is_empty() && shown.iter().all(|m| self.selected.contains(&m.folder));
        let folders: Vec<String> = shown.iter().map(|m| m.folder.clone()).collect();
        let name = |text: &'static str| {
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(text)
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .h(px(44.))
            .px_4()
            .bg(theme.muted)
            .border_b_1()
            .border_color(theme.border)
            .child(
                Checkbox::new("mods-all")
                    .cursor_pointer()
                    .checked(all)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        for folder in &folders {
                            if *on {
                                this.selected.insert(folder.clone());
                            } else {
                                this.selected.remove(folder);
                            }
                        }
                        cx.notify();
                    })),
            )
            .child(name("Mod").flex_1())
            .child(name("Version").w(VERSION))
            .child(name("Actions").w(ACTIONS).flex().justify_end())
    }

    fn row(&self, item: &Mod, last: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let checked = self.selected.contains(&item.folder);
        let picture = match &item.icon {
            Some(path) => div().flex_none().size(ICON).child(
                img(path.clone())
                    .size_full()
                    .rounded(px(8.))
                    .object_fit(ObjectFit::Cover)
                    // Pictures decode as rows scroll into view.
                    .with_loading(|| {
                        Skeleton::new()
                            .size_full()
                            .rounded(px(8.))
                            .into_any_element()
                    }),
            ),
            None => div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(ICON)
                .rounded(px(8.))
                .bg(theme.muted)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Puzzle).size_4()),
        };
        let two = |top: String, bottom: String| {
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(top),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(bottom),
                )
        };
        let author = if item.author.is_empty() {
            item.id.clone()
        } else {
            format!("by {}", item.author)
        };
        // A mod read from `common/`, or a Build 41 one, isn't tied to one version folder.
        let version = item.version.clone().unwrap_or("Any version".into());
        let key = |what: &str| SharedString::from(format!("{what}-{}", item.folder));
        let (id, folder, pick) = (item.id.clone(), item.folder.clone(), item.folder.clone());
        let (dir, mod_id, workshop) = (item.dir.clone(), item.id.clone(), item.workshop);
        div()
            // A uniform list doesn't stretch its rows like a flex column did.
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .when(!last, |row| row.border_b_1().border_color(theme.border))
            .when(checked, |row| row.bg(theme.primary.opacity(0.1)))
            // The table clips with a plain rectangle, so its last row rounds its own corners.
            .when(last, |row| row.rounded_b(theme.radius_lg - px(1.)))
            .child(
                Checkbox::new(key("check"))
                    .cursor_pointer()
                    .checked(checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        if *on {
                            this.selected.insert(pick.clone());
                        } else {
                            this.selected.remove(&pick);
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .child(picture)
                    .child(two(item.name.clone(), author)),
            )
            .child(two(version, item.folder.clone()).w(VERSION))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .w(ACTIONS)
                    .child(
                        Switch::new(key("on"))
                            .cursor_pointer()
                            .checked(item.enabled)
                            .on_click(cx.listener(move |this, on: &bool, window, cx| {
                                this.toggle(&id, *on, window, cx)
                            })),
                    )
                    .child(
                        Button::new(key("remove"))
                            .ghost()
                            .cursor_pointer()
                            .icon(IconName::Trash)
                            .tooltip("Remove")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove(&folder, window, cx)
                            })),
                    )
                    .child(
                        Button::new(key("more"))
                            .ghost()
                            .cursor_pointer()
                            .icon(IconName::EllipsisVertical)
                            .dropdown_menu(move |menu, _, _| {
                                let (dir, mod_id) = (dir.clone(), mod_id.clone());
                                menu.item(
                                    PopupMenuItem::new("Open folder")
                                        .icon(IconName::FolderOpen)
                                        .on_click(move |_, _, cx| cx.open_with_system(&dir)),
                                )
                                .when_some(workshop, |menu, workshop| {
                                    menu.item(
                                        PopupMenuItem::new("Workshop page")
                                            .icon(IconName::ExternalLink)
                                            .on_click(move |_, _, cx| {
                                                cx.open_url(&format!(
                                                    "https://steamcommunity.com/sharedfiles/filedetails/?id={workshop}"
                                                ))
                                            }),
                                    )
                                })
                                .item(
                                    PopupMenuItem::new("Copy mod id")
                                        .icon(IconName::Copy)
                                        .on_click(move |_, window, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                mod_id.clone(),
                                            ));
                                            window.push_notification(
                                                toast::done("Copied! Paste it anywhere"),
                                                cx,
                                            );
                                        }),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    /// What to do with the checked rows; floats over the bottom of the table while any is.
    fn selection(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let chosen = self.chosen();
        let (any_off, any_on) = (
            chosen.iter().any(|m| !m.enabled),
            chosen.iter().any(|m| m.enabled),
        );
        let count = match chosen.len() {
            1 => "1 mod selected".to_owned(),
            many => format!("{many} mods selected"),
        };
        let act = |id: &'static str, label: &'static str, icon: IconName, enabled: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1p5()
                .h(px(32.))
                .px_2p5()
                .rounded(theme.radius)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(Icon::new(icon).size_4())
                .child(label)
                .map(|this| {
                    if enabled {
                        this.cursor_pointer().hover(|style| style.bg(theme.muted))
                    } else {
                        this.opacity(0.5)
                    }
                })
        };
        let rule = || div().w(px(1.)).h(px(20.)).bg(theme.border);
        div()
            .occlude()
            .flex()
            .items_center()
            .gap_2()
            .h(px(48.))
            .pl_4()
            .pr_2()
            .rounded(theme.radius_lg)
            .bg(theme.tokens.popover)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(count),
            )
            .child(rule())
            .child(
                act("selection-clear", "Clear", IconName::Close, true).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.selected.clear();
                        cx.notify();
                    },
                )),
            )
            .child(div().w_6())
            .child(
                act("selection-on", "Turn on", IconName::Check, any_off).when(any_off, |this| {
                    this.on_click(
                        cx.listener(|this, _, window, cx| this.toggle_chosen(true, window, cx)),
                    )
                }),
            )
            .child(
                act("selection-off", "Turn off", IconName::Minus, any_on).when(any_on, |this| {
                    this.on_click(
                        cx.listener(|this, _, window, cx| this.toggle_chosen(false, window, cx)),
                    )
                }),
            )
            .child(rule())
            .child(
                act("selection-remove", "Remove", IconName::Trash, true)
                    .text_color(theme.danger)
                    .on_click(cx.listener(|this, _, window, cx| this.remove_chosen(window, cx))),
            )
    }
}

impl Render for Mods {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = self.scroll.0.borrow().base_handle.clone();
        ui::smooth_scroll("mods-scroll", &base, window, cx);
        let adding = self
            .shell
            .read_with(cx, |shell, _| shell.adding_mods(&self.instance.id))
            .unwrap_or(false);
        let shown: Vec<Mod> = self.shown(cx).into_iter().cloned().collect();
        // No mods yet: the page says so and offers the one thing to do. A search, a sort and a
        // filter with nothing to work on would only be noise.
        if self.mods.is_empty() {
            let (instance, shell) = (self.instance.clone(), self.shell.clone());
            let theme = cx.theme();
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .items_center()
                .gap_2()
                .pt(px(120.))
                .when(adding, |this| {
                    this.child(div().w(px(200.)).pb_4().child(Loader))
                })
                .child(
                    Icon::new(IconName::Puzzle)
                        .size_8()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("No mods yet"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Add one from the Workshop and it lands here."),
                )
                .child(
                    div().pt_3().child(
                        FancyButton::new("mod-add-first", "Add mod")
                            .small()
                            .icon(IconName::Plus)
                            .on_click(move |_, window, cx| {
                                super::mod_add::open(instance.clone(), shell.clone(), window, cx)
                            }),
                    ),
                )
                .into_any_element();
        }
        let list = if shown.is_empty() {
            Empty::new(
                IconName::Search,
                "No mods match",
                "Try another name, or change the filter.",
            )
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .min_h_0()
                .rounded(cx.theme().radius_lg)
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .overflow_hidden()
                .child(self.header(&shown, cx))
                // Only the rows in view are built: a collection brings hundreds of mods, each
                // with a picture to decode.
                .child(
                    uniform_list(
                        "mods-list",
                        shown.len(),
                        cx.processor(|this, range: Range<usize>, _, cx| {
                            let shown = this.shown(cx);
                            let last = shown.len().saturating_sub(1);
                            let picked: Vec<(usize, Mod)> = range
                                .filter_map(|ix| shown.get(ix).map(|m| (ix, (*m).clone())))
                                .collect();
                            picked
                                .iter()
                                .map(|(ix, item)| this.row(item, *ix == last, cx))
                                .collect()
                        }),
                    )
                    // Rows times their height, capped by the room left: the table hugs a short
                    // list and scrolls a long one. The default sizing gives it no height at all.
                    .with_sizing_behavior(ListSizingBehavior::Infer)
                    .track_scroll(&self.scroll)
                    .min_h_0(),
                )
                .into_any_element()
        };
        let on = self.mods.iter().filter(|m| m.enabled).count();
        // Floats over the bottom of the table; the bar inside is what rises in.
        let bar = (!self.selected.is_empty()).then(|| {
            div()
                .absolute()
                // Above where toasts come up, so what an action says doesn't cover the bar.
                .bottom(px(88.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(ui::enter(self.selection(cx), "mods-selection"))
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_4()
            .child(self.toolbar(cx))
            .when(adding, |this| this.child(Loader))
            .when(!self.mods.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{on} of {} on", self.mods.len())),
                )
            })
            .child(list)
            .children(bar)
            .into_any_element()
    }
}
