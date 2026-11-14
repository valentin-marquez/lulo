//! The panel on the right: what is new in Project Zomboid (the developers' announcements) and
//! the mods trending on the Workshop this week, with a way to add one to an instance.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use steam::news::Post;
use steam::workshop::Item;

use crate::instance::{APP, branches, mods};
use crate::shell::Shell;

pub const WIDTH: Pixels = px(320.);
/// Below this window width the panel is left out: the page in the middle needs the room.
pub const MIN_WINDOW: Pixels = px(1200.);

/// Room for a trending mod's title: the panel less its padding, the row's, the picture, the
/// button and the gaps between them.
const TITLE: Pixels = px(176.);

const POSTS: u32 = 5;
const MODS: u32 = 6;
/// "Trending" looks at the last week.
const DAYS: u32 = 7;

/// A trending mod with its Workshop picture, when it could be fetched.
#[derive(Debug, Clone)]
pub struct Trending {
    pub item: Item,
    pub picture: Option<PathBuf>,
}

/// What the panel shows. Each half is empty when Steam couldn't be reached for it.
#[derive(Debug, Clone, Default)]
pub struct Feed {
    pub posts: Vec<Post>,
    pub mods: Vec<Trending>,
    /// A screenshot of the game for the Library's banner.
    pub banner: Option<PathBuf>,
    /// Another one, to open this panel.
    pub cover: Option<PathBuf>,
}

/// Which of the store page's screenshots go where, by their place on the page: the snowy
/// lakeside house for the banner (its blues sit with the accent), the suburb at night here.
/// The page's first one stands in when Steam reorders them.
const BANNER_SHOT: usize = 2;
const COVER_SHOT: usize = 0;

/// Where the screenshots are kept, next to the instances.
fn cache() -> PathBuf {
    crate::instance::store::root()
        .with_file_name("cache")
        .join("game")
}

/// The file that names the last banner, so the next start shows it before Steam answers.
fn banner_note() -> PathBuf {
    cache().join("banner")
}

/// The banner the last run showed, if it is still in the cache. One small read, fine on the
/// UI thread at start.
pub fn cached_banner() -> Option<PathBuf> {
    let name = std::fs::read_to_string(banner_note()).ok()?;
    let name = name.trim();
    // A plain file name in the cache, never a path out of it.
    if std::path::Path::new(name).file_name() != Some(std::ffi::OsStr::new(name)) {
        return None;
    }
    let path = cache().join(name);
    path.is_file().then_some(path)
}

/// The game's screenshots from its Steam store page. They are fetched for whoever runs lulo
/// and kept in its cache, never shipped with it; and they are scenes of the game, not its
/// logo or its mascot, which lulo doesn't show (it is not an official tool).
async fn screenshot(http: &steam::reqwest::Client, urls: &[String], ix: usize) -> Option<PathBuf> {
    let url = urls.get(ix).or(urls.first())?;
    let cache = cache();
    // Named after the file on Steam, which changes when the picture does.
    let name: String = url
        .split('?')
        .next()?
        .rsplit('/')
        .next()?
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
        .collect();
    if name.is_empty() || name.starts_with('.') {
        return None;
    }
    let path = cache.join(name);
    if path.is_file() {
        return Some(path);
    }
    let bytes = steam::workshop::preview(http, url).await.ok()?;
    std::fs::create_dir_all(&cache).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Asks Steam for the news and the trending mods. Either can fail on its own.
pub async fn load() -> Feed {
    crate::runtime::spawn(async {
        let http = steam::http();
        let posts = steam::news::announcements(&http, APP, POSTS)
            .await
            .unwrap_or_default();
        let mut found = Vec::new();
        if let Ok(mut client) = steam::Client::anonymous(&http).await {
            let items = steam::workshop::trending(&mut client, APP, DAYS, MODS)
                .await
                .unwrap_or_default();
            for item in items {
                let picture = mods::picture(&http, item.preview.as_deref()).await;
                found.push(Trending { item, picture });
            }
        }
        let shots = steam::store::screenshots(&http, APP)
            .await
            .unwrap_or_default();
        let banner = screenshot(&http, &shots, BANNER_SHOT).await;
        if let Some(name) = banner.as_deref().and_then(|path| path.file_name()) {
            let _ = std::fs::write(banner_note(), name.as_encoded_bytes());
        }
        Feed {
            posts,
            mods: found,
            banner,
            cover: screenshot(&http, &shots, COVER_SHOT).await,
        }
    })
    .await
    .unwrap_or_default()
}

/// `27557` as `27.5K`: how many people use a mod, at a glance.
fn compact(n: u32) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}.{}K", n / 1_000, n % 1_000 / 100),
        _ => format!("{}.{}M", n / 1_000_000, n % 1_000_000 / 100_000),
    }
}

/// A placeholder that fills its box while what goes there loads.
fn shimmer(radius: Pixels) -> Skeleton {
    Skeleton::new().size_full().rounded(radius)
}

/// The panel's shape before Steam answers: the lead picture, a few headlines and the trending
/// rows, each at its real size, so nothing moves when the feed lands.
fn waiting(cx: &App) -> Div {
    let theme = cx.theme();
    let line = |width: f32| {
        Skeleton::new()
            .h(px(12.))
            .w(relative(width))
            .rounded(px(4.))
    };
    let posts = (0..POSTS - 1).map(move |ix| {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2()
            .child(line(if ix % 2 == 0 { 0.9 } else { 0.7 }))
            .child(line(0.3))
    });
    let mods = (0..MODS).map(move |_| {
        div()
            .flex()
            .items_center()
            .gap_3()
            .p_2()
            .child(div().flex_none().size(px(44.)).child(shimmer(px(8.))))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1p5()
                    .child(line(0.8))
                    .child(line(0.4)),
            )
    });
    div()
        .flex()
        .flex_col()
        .child(heading("NEWS", cx))
        .child(
            div()
                .h(px(166.))
                .mx_2()
                .mb_2()
                .mt_1()
                .child(shimmer(theme.radius_lg)),
        )
        .children(posts)
        .child(div().h_3())
        .child(heading("TRENDING MODS THIS WEEK", cx))
        .children(mods)
}

fn heading(text: &'static str, cx: &App) -> Div {
    div()
        .px_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn note(text: &'static str, cx: &App) -> Div {
    div()
        .px_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// The newest announcement as the panel's lead: its title over a scene of the game, darkened
/// at the bottom so the words read. Without the picture (offline) it is a plain item.
fn lead(newest: &Post, cover: Option<PathBuf>, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let Some(cover) = cover else {
        return post(0, newest, cx);
    };
    let url = newest.url.clone();
    let radius = theme.radius_lg;
    div()
        .id("post-lead")
        .relative()
        .flex_none()
        // A screenshot's own 16:9 at the panel's width.
        .h(px(166.))
        .mx_2()
        .mb_2()
        .rounded(theme.radius_lg)
        .overflow_hidden()
        .cursor_pointer()
        // The frame's clip is a plain rectangle, so picture and shade round their own corners.
        .child(
            img(cover)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(theme.radius_lg)
                .object_fit(ObjectFit::Cover)
                .with_loading(move || shimmer(radius).into_any_element()),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(theme.radius_lg)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(black().opacity(0.), 0.15),
                    linear_color_stop(black().opacity(0.82), 1.),
                )),
        )
        .child(
            div()
                .absolute()
                .left_3()
                .right_3()
                .bottom_3()
                .flex()
                .flex_col()
                .gap_0p5()
                .text_color(white())
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .opacity(0.85)
                        .child(format!("Latest, {}", branches::date(newest.date))),
                )
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .line_clamp(2)
                        .child(newest.title.clone()),
                ),
        )
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// One announcement: opens its page in the browser.
fn post(ix: usize, post: &Post, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let url = post.url.clone();
    div()
        .id(("post", ix))
        .flex()
        .flex_col()
        .gap_0p5()
        .p_2()
        .rounded(theme.radius)
        .cursor_pointer()
        .hover(|row| row.bg(theme.sidebar_accent))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .line_clamp(2)
                .child(post.title.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(branches::date(post.date)),
        )
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// One trending mod: picture, name, how many use it, and a menu to add it to an instance.
fn trending(
    ix: usize,
    found: &Trending,
    shell: &Shell,
    handle: &WeakEntity<Shell>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let item = &found.item;
    let picture = match &found.picture {
        Some(path) => div().flex_none().size(px(44.)).child(
            img(path.clone())
                .size_full()
                .rounded(px(8.))
                .object_fit(ObjectFit::Cover)
                .with_loading(|| shimmer(px(8.)).into_any_element()),
        ),
        None => div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(44.))
            .rounded(px(8.))
            .bg(theme.muted)
            .text_color(theme.muted_foreground)
            .child(Icon::new(IconName::Puzzle).size_4()),
    };
    let any = !shell.instances().is_empty();
    let (item_for_pick, handle) = (item.clone(), handle.clone());
    div()
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .child(picture)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        // A set width, so the text wraps inside it and ends in an ellipsis
                        // instead of running under the button and being cut mid-word.
                        .w(TITLE)
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        // Workshop titles are long; one line cut most of them short.
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(item.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{} playing with it", compact(item.subscriptions))),
                ),
        )
        .when(any, |row| {
            row.child(
                Button::new(("trending-add", ix))
                    .ghost()
                    .cursor_pointer()
                    .icon(IconName::Download)
                    .tooltip("Add to an instance")
                    .on_click(move |_, window, cx| {
                        pick(item_for_pick.clone(), handle.clone(), window, cx)
                    }),
            )
        })
}

/// Asks which instance gets `item`, offering only the ones it is made for: a Build 42 mod
/// in a Build 41 instance would just not show up in the game.
fn pick(item: Item, shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
    use gpui_kit::component::WindowExt;
    use gpui_kit::component::dialog::DialogEntrance;
    let Ok(instances) = shell.read_with(cx, |shell, _| shell.instances().to_vec()) else {
        return;
    };
    let (fitting, other): (Vec<_>, Vec<_>) = instances
        .into_iter()
        .partition(|instance| mods::fits(instance, &item.tags));
    window.open_dialog(cx, move |dialog, _, cx| {
        let theme = cx.theme().clone();
        let rows = fitting.iter().map(|instance| {
            let (shell, instance_for_add, id) = (shell.clone(), instance.clone(), item.id);
            div()
                .id(SharedString::from(format!("pick-{}", instance.id)))
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .rounded(theme.radius)
                .cursor_pointer()
                .hover(|row| row.bg(theme.muted))
                .child(crate::instance::art::art(instance, 40., px(8.), cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(instance.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(instance.branch.clone()),
                        ),
                )
                .on_click(move |_, window, cx| {
                    window.close_dialog(cx);
                    let (instance, handle) = (instance_for_add.clone(), window.window_handle());
                    let _ = shell.update(cx, |shell, cx| shell.add_mod(instance, id, handle, cx));
                })
        });
        let hidden = match other.len() {
            0 => None,
            1 => Some("1 instance runs another build, so it isn't offered".to_owned()),
            many => Some(format!(
                "{many} instances run another build, so they aren't offered"
            )),
        };
        dialog
            .entrance(DialogEntrance::FadeSlide)
            .title("Add to an instance")
            .centered()
            .w(px(420.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(item.title.clone()),
                    )
                    .when(fitting.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .child("None of your instances runs the build this mod is for."),
                        )
                    })
                    .child(div().flex().flex_col().gap_1().children(rows))
                    .children(hidden.map(|text| {
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(text)
                    })),
            )
    });
}

/// The panel. `feed` is `None` while Steam is being asked.
pub fn render(
    shell: &Shell,
    feed: Option<&Feed>,
    scroll: &ScrollHandle,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let handle = cx.entity().downgrade();
    let panel = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(WIDTH)
        .h_full()
        .p_3()
        .gap_1();
    let Some(feed) = feed else {
        return panel.child(waiting(cx));
    };
    let news = match feed.posts.split_first() {
        None => div().child(note("No news got through. Steam may be napping", cx)),
        Some((newest, older)) => div()
            .flex()
            .flex_col()
            .child(lead(newest, feed.cover.clone(), cx))
            .children(
                older
                    .iter()
                    .enumerate()
                    .map(|(ix, found)| post(ix + 1, found, cx)),
            ),
    };
    let trending_mods = match feed.mods.is_empty() {
        true => div().child(note("The Workshop didn't answer", cx)),
        false => div().flex().flex_col().children(
            feed.mods
                .iter()
                .enumerate()
                .map(|(ix, found)| trending(ix, found, shell, &handle, cx)),
        ),
    };
    panel
        .child(heading("NEWS", cx))
        .child(news.flex_none())
        .child(div().h_3().flex_none())
        .child(heading("TRENDING MODS THIS WEEK", cx))
        // The list scrolls on its own, in what is left of the panel: the news above stay put.
        .child(
            div()
                .id("discover-trending")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(trending_mods),
        )
}

#[cfg(test)]
mod tests {
    use super::compact;

    #[test]
    fn counts_read_at_a_glance() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(687), "687");
        assert_eq!(compact(4_881), "4.8K");
        assert_eq!(compact(27_557), "27.5K");
        assert_eq!(compact(1_250_000), "1.2M");
    }
}
