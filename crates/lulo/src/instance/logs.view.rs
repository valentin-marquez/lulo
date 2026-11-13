//! Logs tab: one bar (session, level counts, search) over the console, followed live. Lines are
//! selected by row (click, drag or shift-click) and copied with the shortcut or the Copy button.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::logs::{self, Console, Level, Line, Session};
use crate::ui::Empty;

const FILTERS: [(&str, Option<Level>); 4] = [
    ("All", None),
    ("Error", Some(Level::Error)),
    ("Warn", Some(Level::Warn)),
    ("Info", Some(Level::Info)),
];

pub struct Logs {
    cachedir: PathBuf,
    session: Session,
    console: Console,
    lines: Vec<Line>,
    /// Indices into `lines` that pass the level filter and the search.
    shown: Vec<usize>,
    filter: usize,
    search: Entity<InputState>,
    scroll: UniformListScrollHandle,
    /// Selected rows of `shown`: where the selection started and where it ends now.
    selection: Option<(usize, usize)>,
    dragging: bool,
    focus: FocusHandle,
    _poll: Task<()>,
    _search: Subscription,
}

impl Logs {
    pub fn new(cachedir: &Path, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let on_search = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.refilter(cx);
            }
        });
        // ponytail: polls the file twice a second; a file watcher only if this shows up in a
        // profile.
        let poll = cx.spawn(async move |this, cx| {
            while this.update(cx, |this, cx| this.poll(cx)).is_ok() {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
            }
        });
        let session = logs::sessions(cachedir).remove(0);
        Self {
            cachedir: cachedir.to_owned(),
            console: Console::new(session.path.clone()),
            session,
            lines: Vec::new(),
            shown: Vec::new(),
            filter: 0,
            search,
            scroll: UniformListScrollHandle::new(),
            selection: None,
            dragging: false,
            focus: cx.focus_handle(),
            _poll: poll,
            _search: on_search,
        }
    }

    fn open(&mut self, session: Session, cx: &mut Context<Self>) {
        self.console = Console::new(session.path.clone());
        self.session = session;
        self.lines.clear();
        self.poll(cx);
        self.refilter(cx);
        self.scroll.scroll_to_bottom();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let (new, restarted) = self.console.poll();
        if restarted {
            self.lines.clear();
        } else if new.is_empty() {
            return;
        }
        // Stay at the bottom while following; leave the view alone if the user scrolled up.
        let follow = self.at_end();
        self.lines.extend(new);
        if restarted {
            self.refilter(cx);
        } else {
            // Appending keeps the selection: rows already shown don't move.
            let selection = self.selection;
            self.refilter(cx);
            self.selection = selection;
        }
        if follow {
            self.scroll.scroll_to_bottom();
        }
    }

    fn at_end(&self) -> bool {
        self.scroll.is_scrolled_to_end().unwrap_or(true)
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().trim().to_lowercase()
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        let level = FILTERS[self.filter].1;
        self.shown = self
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| level.is_none_or(|level| line.level == level))
            .filter(|(_, line)| query.is_empty() || line.raw.to_lowercase().contains(&query))
            .map(|(ix, _)| ix)
            .collect();
        self.selection = None;
        cx.notify();
    }

    /// Entries per level; continuation lines count with their entry, not on their own.
    fn count(&self, level: Option<Level>) -> usize {
        self.lines
            .iter()
            .filter(|line| line.time.is_some() && level.is_none_or(|l| line.level == l))
            .count()
    }

    fn selected(&self) -> Option<Range<usize>> {
        let (a, b) = self.selection?;
        Some(a.min(b)..a.max(b) + 1)
    }

    /// The selected lines, or every shown line when nothing is selected.
    fn copy(&self, cx: &mut App) {
        let rows = self.selected().unwrap_or(0..self.shown.len());
        let text: Vec<&str> = self.shown[rows]
            .iter()
            .map(|&ix| self.lines[ix].raw.as_ref())
            .collect();
        cx.write_to_clipboard(ClipboardItem::new_string(text.join("\n")));
    }

    fn row(&self, ix: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let line = &self.lines[self.shown[ix]];
        let color = match line.level {
            Level::Error => theme.danger,
            Level::Warn => theme.warning,
            Level::Info => theme.foreground,
            Level::Debug => theme.muted_foreground,
        };
        let selected = self.selected().is_some_and(|rows| rows.contains(&ix));
        let entry = line.time.is_some();
        let cell = |width: f32| div().flex_none().w(px(width)).truncate();
        // ponytail: long lines are cut at the edge; wrap or scroll sideways if people miss it.
        div()
            .id(ix)
            .flex()
            .items_center()
            .gap_3()
            .h(px(22.))
            .px_3()
            .when(selected, |this| this.bg(theme.selection))
            .when(!selected, |this| this.hover(|s| s.bg(theme.muted)))
            .child(
                cell(60.)
                    .text_color(theme.muted_foreground)
                    .children(line.time.clone()),
            )
            .child(
                cell(44.)
                    .text_color(color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(entry, |this| this.child(line.level.label())),
            )
            .child(
                cell(72.)
                    .text_color(theme.muted_foreground)
                    .children(line.category.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(match line.level {
                        Level::Info => theme.foreground,
                        _ => color,
                    })
                    .child(line.message.clone()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.selection = match this.selection {
                        Some((anchor, _)) if event.modifiers.shift => Some((anchor, ix)),
                        _ => Some((ix, ix)),
                    };
                    this.dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                if this.dragging
                    && let Some((anchor, head)) = this.selection
                    && head != ix
                {
                    this.selection = Some((anchor, ix));
                    cx.notify();
                }
            }))
    }

    fn session_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (this, cachedir, current) = (
            cx.entity().downgrade(),
            self.cachedir.clone(),
            self.session.path.clone(),
        );
        Button::new("log-session")
            .outline()
            .small()
            .cursor_pointer()
            .icon(if self.session.live {
                Icon::new(IconName::Radio)
            } else {
                // Not in gpui-kit's catalog; served by our own icon set.
                Icon::empty().path("icons/history.svg")
            })
            .label(self.session.name.clone())
            .dropdown_menu(move |mut menu, _, _| {
                // Listed on every open, so sessions saved since show up.
                for session in logs::sessions(&cachedir) {
                    let this = this.clone();
                    let checked = session.path == current;
                    menu = menu.item(
                        PopupMenuItem::new(session.name.clone())
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let _ = this.update(cx, |logs, cx| logs.open(session.clone(), cx));
                            }),
                    );
                }
                menu
            })
    }

    /// The level filter, each with its count; the raised pill slides to the chosen one.
    fn levels(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        TabBar::new("log-levels")
            .segmented()
            .small()
            .selected_index(self.filter)
            .children(FILTERS.iter().enumerate().map(|(ix, (label, level))| {
                let active = ix == self.filter;
                Tab::new().label(*label).cursor_pointer().suffix(
                    div()
                        .text_xs()
                        .text_color(match (active, level) {
                            (true, Some(Level::Error)) => theme.danger,
                            (true, Some(Level::Warn)) => theme.warning,
                            (true, _) => theme.primary,
                            _ => theme.muted_foreground,
                        })
                        .child(self.count(*level).to_string()),
                )
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.filter = *ix;
                this.refilter(cx);
            }))
    }

    /// Why the list is empty: nothing written yet, or nothing passes the filter.
    fn empty(&self, cx: &App) -> Empty {
        if self.lines.is_empty() {
            return match self.session.live {
                true => Empty::new(
                    IconName::SquareTerminal,
                    "Nothing logged yet",
                    "Start the instance to follow its console here.",
                ),
                false => Empty::new(
                    IconName::SquareTerminal,
                    "Empty session",
                    "This log file has no lines.",
                ),
            };
        }
        let query = self.query(cx);
        if !query.is_empty() {
            return Empty::new(
                IconName::Search,
                format!("No lines match \"{query}\""),
                "Try another word, or All levels.",
            );
        }
        let what = match FILTERS[self.filter].1 {
            Some(Level::Error) => "No errors",
            Some(Level::Warn) => "No warnings",
            _ => "No info lines",
        };
        Empty::new(
            IconName::Check,
            format!("{what} in this session"),
            "Pick All to see every line.",
        )
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let path = self.console.path().to_owned();
        let selected = self.selected().map(|rows| rows.len());
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(format!("{} lines", self.shown.len()))
                    .children(selected.map(|n| format!("{n} selected"))),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("log-copy")
                            .ghost()
                            .xsmall()
                            .cursor_pointer()
                            .icon(IconName::Copy)
                            .label(match selected {
                                Some(_) => "Copy selection",
                                None => "Copy all",
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(
                        Button::new("log-open")
                            .ghost()
                            .xsmall()
                            .cursor_pointer()
                            .icon(IconName::ExternalLink)
                            .label("Open file")
                            .on_click(move |_, _, cx| cx.open_with_system(&path)),
                    ),
            )
    }
}

impl Render for Logs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let list = if self.shown.is_empty() {
            self.empty(cx).into_any_element()
        } else {
            div()
                .id("log-view")
                .track_focus(&self.focus)
                .relative()
                .flex_1()
                .min_h_0()
                .py_2()
                .rounded(theme.radius_lg)
                .bg(theme.group_box)
                .border_1()
                .border_color(theme.border)
                .font_family(theme.mono_font_family.clone())
                .text_xs()
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    let key = event.keystroke.key.as_str();
                    if key == "c" && event.keystroke.modifiers.secondary(cx) {
                        this.copy(cx);
                    } else if key == "escape" && this.selection.take().is_some() {
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.dragging = false),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.dragging = false),
                )
                // Scrolling decides whether we follow and whether "Latest" shows.
                .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                .child(
                    uniform_list(
                        "log-lines",
                        self.shown.len(),
                        cx.processor(|this, range: Range<usize>, _, cx| {
                            range.map(|ix| this.row(ix, cx)).collect()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                )
                .vertical_scrollbar(&self.scroll)
                .when(!self.at_end(), |this| {
                    this.child(
                        div().absolute().bottom_3().right_5().child(
                            Button::new("log-latest")
                                .small()
                                .cursor_pointer()
                                .icon(Icon::new(IconName::ArrowDown))
                                .label("Latest")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.scroll.scroll_to_bottom();
                                    cx.notify();
                                })),
                        ),
                    )
                })
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.session_picker(cx))
                    .child(self.levels(cx))
                    .child(div().flex_1())
                    .child(
                        div()
                            .w(px(240.))
                            .child(Input::new(&self.search).small().cleanable(true)),
                    ),
            )
            .child(list)
            .child(self.footer(cx))
    }
}
