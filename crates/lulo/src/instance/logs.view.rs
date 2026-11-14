//! Logs tab: one bar (session, level counts, search) over the console, followed live. Lines are
//! selected by row (click, drag, shift-click, or all of them with the select-all shortcut) and
//! copied with the copy shortcut or the Copy button.
//! Dragging past the top or bottom edge scrolls the list and keeps selecting.

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

/// Height of the bar's controls: the primary button's small size, like every toolbar.
const CONTROL: Pixels = px(36.);
/// The widest the session picker gets; a longer name is cut short instead of pushing the bar.
const SESSION: Pixels = px(168.);
/// Height of one line.
const ROW: f32 = 22.;
/// Dragging this close to the list's top or bottom edge, or past it, scrolls it.
const EDGE: f32 = 24.;

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
    /// Runs while the mouse button is held on the list: follows the pointer, scrolling at
    /// the edges. Dropping it ends the drag.
    drag: Option<Task<()>>,
    /// Whether any line carries a time; Build 42's live console has none, and then the
    /// column is left out instead of standing empty.
    timed: bool,
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
            drag: None,
            timed: false,
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
        self.timed = self.lines.iter().any(|line| line.time.is_some());
        self.selection = None;
        cx.notify();
    }

    /// The shown row under window position `y`, clamped to the list.
    fn row_at(&self, y: Pixels) -> Option<usize> {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let inside = y - handle.bounds().top() - handle.offset().y;
        let row = (f32::from(inside) / ROW).floor().max(0.) as usize;
        Some(row.min(self.shown.len().checked_sub(1)?))
    }

    /// One step of a drag: scroll if the pointer is at an edge, then select up to the row
    /// under it.
    fn drag_to(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let (bounds, offset) = (handle.bounds(), handle.offset());
        let (above, below) = (
            f32::from(bounds.top() + px(EDGE) - pointer.y),
            f32::from(pointer.y - (bounds.bottom() - px(EDGE))),
        );
        // Faster the further past the edge, a few lines per tick at most.
        let step = |past: f32| px((past / EDGE).clamp(0.2, 4.) * ROW);
        let wanted = if above > 0. {
            offset.y + step(above)
        } else if below > 0. {
            offset.y - step(below)
        } else {
            offset.y
        };
        let wanted = wanted.clamp(-handle.max_offset().y, px(0.));
        if wanted != offset.y {
            handle.set_offset(point(offset.x, wanted));
            cx.notify();
        }
        if let (Some((anchor, head)), Some(row)) = (self.selection, self.row_at(pointer.y))
            && head != row
        {
            self.selection = Some((anchor, row));
            cx.notify();
        }
    }

    /// Entries per level; continuation lines count with their entry, not on their own.
    fn count(&self, level: Option<Level>) -> usize {
        self.lines
            .iter()
            .filter(|line| line.category.is_some() && level.is_none_or(|l| line.level == l))
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
            Level::Warn => crate::ui::warning_text(cx),
            Level::Info => theme.foreground,
            Level::Debug => theme.muted_foreground,
        };
        let selected = self.selected().is_some_and(|rows| rows.contains(&ix));
        let entry = line.category.is_some();
        let cell = |width: f32| div().flex_none().w(px(width)).truncate();
        // ponytail: long lines are cut at the edge; wrap or scroll sideways if people miss it.
        div()
            .id(ix)
            .flex()
            .items_center()
            .gap_3()
            .h(px(ROW))
            // The whole width, so the hover and selection fills don't stop where the text does.
            .w_full()
            .px_3()
            .when(selected, |this| this.bg(theme.selection))
            .when(!selected, |this| this.hover(|s| s.bg(theme.muted)))
            .when(self.timed, |this| {
                this.child(
                    cell(60.)
                        .text_color(theme.muted_foreground)
                        .children(line.time.clone()),
                )
            })
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
                    // Follows the pointer from a timer, not from each row's mouse-move: that
                    // only fires over a row, so the drag stalled at the list's edges and
                    // never scrolled.
                    this.drag = Some(cx.spawn_in(window, async move |this, cx| {
                        loop {
                            cx.background_executor()
                                .timer(Duration::from_millis(16))
                                .await;
                            let alive = this.update_in(cx, |this, window, cx| {
                                this.drag_to(window.mouse_position(), cx)
                            });
                            if alive.is_err() {
                                break;
                            }
                        }
                    }));
                    cx.notify();
                }),
            )
    }

    fn session_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (this, cachedir, current) = (
            cx.entity().downgrade(),
            self.cachedir.clone(),
            self.session.path.clone(),
        );
        // In a tray of its own, like the level filter beside it, so it reads as a control
        // and not as a loose label.
        let picker = Button::new("log-session")
            .ghost()
            // Two px shorter than the tray, so its hover fill sits inside the border.
            .h(CONTROL - px(4.))
            .max_w(SESSION - px(2.))
            .overflow_hidden()
            .px_3()
            .rounded(cx.theme().radius - px(2.))
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
            });
        // The tray carries the fill and border; the button inside keeps the usual ghost hover,
        // which painting a background on the button itself fought with.
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(CONTROL)
            .max_w(SESSION)
            .px(px(1.))
            .rounded(cx.theme().radius)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .child(picker)
    }

    /// The level filter, each with its count, as pills in a tray like the instance's tabs:
    /// the chosen one filled with the accent, the fill sliding between them.
    fn levels(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // Pills of 26 px in the tray's 4 px of padding and its border: the bar's 36 px.
        let tabs = TabBar::new("log-levels")
            .pill()
            .selected_index(self.filter)
            .children(FILTERS.iter().enumerate().map(|(ix, (label, level))| {
                let active = ix == self.filter;
                // The count right after the name, inside the pill and in a quiet color. As the
                // tab's suffix it sat against the pill's edge, where a close button goes.
                let ink = if active {
                    theme.primary_foreground.opacity(0.8)
                } else {
                    theme.muted_foreground
                };
                Tab::new().cursor_pointer().child(
                    div().flex().items_baseline().gap_1p5().child(*label).child(
                        div()
                            .text_xs()
                            .text_color(ink)
                            .child(self.count(*level).to_string()),
                    ),
                )
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.filter = *ix;
                this.refilter(cx);
            }));
        // Exactly the bar's height, whatever the pills measure, so it lines up with the
        // session picker and the search field beside it.
        crate::ui::tab_tray(tabs, cx)
            .flex()
            .items_center()
            .h(CONTROL)
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
                            .small()
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
                            .small()
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
                    let command = event.keystroke.modifiers.secondary(cx);
                    if key == "c" && command {
                        this.copy(cx);
                    } else if key == "a" && command && !this.shown.is_empty() {
                        // Every shown line: what the filter and the search left.
                        this.selection = Some((0, this.shown.len() - 1));
                        cx.notify();
                    } else if key == "escape" && this.selection.take().is_some() {
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.drag = None),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.drag = None),
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
                    .child(
                        // The input's own frame only comes in 32 or 44 px, so ours carries
                        // the border at the bar's height and the input sits inside it bare.
                        div()
                            .flex_1()
                            .min_w(px(160.))
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
                    ),
            )
            .child(list)
            .child(self.footer(cx))
    }
}
