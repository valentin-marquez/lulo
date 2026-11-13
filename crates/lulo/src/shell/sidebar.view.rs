//! Left panel: pages, every instance with its state, then new instance, settings and account.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Page, Shell};
use crate::account::Account;
use crate::instance::Status;

const WIDTH: Pixels = px(248.);

fn row(id: impl Into<ElementId>, active: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .rounded(theme.radius)
        .border_1()
        .cursor_pointer()
        .map(|this| {
            if active {
                this.bg(theme.group_box).border_color(theme.border)
            } else {
                this.border_color(transparent_black())
                    .hover(|s| s.bg(theme.sidebar_accent))
            }
        })
}

fn tile(icon: IconName, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(34.))
        .rounded(px(8.))
        .bg(theme.muted)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size_4())
}

/// The account's tile: their Steam picture, or the generic icon when they have none.
fn account_tile(account: Option<&Account>, cx: &App) -> Div {
    match account.and_then(Account::avatar) {
        Some(path) => div()
            .flex_none()
            .size(px(34.))
            .rounded(px(8.))
            .overflow_hidden()
            // The picture needs its own radius: the frame's clip is a plain rectangle.
            .child(
                img(path)
                    .size_full()
                    .rounded(px(8.))
                    .object_fit(ObjectFit::Cover),
            ),
        None => tile(IconName::CircleUser, cx),
    }
}

fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .truncate()
        .child(text.into())
}

fn status(status: Status, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .truncate()
        .child(status.label())
}

fn nav(
    page: Page,
    icon: IconName,
    text: &'static str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    row(text, shell.page == page, cx)
        .child(tile(icon, cx))
        .child(label(text))
        .on_click(cx.listener(move |shell, _, _, cx| shell.go(page, cx)))
}

pub fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let handle = cx.entity().downgrade();
    let home = nav(Page::Library, IconName::Library, "Library", shell, cx);
    let screenshots = nav(Page::Screenshots, IconName::Image, "Screenshots", shell, cx);
    let theme = cx.theme();

    let instances = shell.instances.iter().enumerate().map(|(ix, instance)| {
        row(instance.id.clone(), shell.page == Page::Instance(ix), cx)
            .child(crate::instance::art::art(instance, 34., px(8.), cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(label(instance.name.clone()))
                    .child(status(shell.status(instance), cx)),
            )
            .on_click(cx.listener(move |shell, _, _, cx| shell.go(Page::Instance(ix), cx)))
    });

    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(WIDTH)
        .px_2()
        .py_2()
        .gap_1()
        .bg(theme.sidebar)
        .child(home)
        .child(screenshots)
        .child(
            div()
                .flex()
                .justify_between()
                .px_2()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child("INSTANCES")
                .child(shell.instances.len().to_string()),
        )
        .children(instances)
        .child(div().h(px(1.)).mx_2().my_1().bg(theme.border))
        .child(
            row("new", false, cx)
                .child(tile(IconName::Plus, cx))
                .child(label("New instance"))
                .on_click({
                    let handle = handle.clone();
                    move |_, window, cx| Shell::create(handle.clone(), window, cx)
                }),
        )
        .child(div().flex_1())
        .child(
            row("settings", false, cx)
                .child(tile(IconName::Settings, cx))
                .child(div().text_sm().child("Settings"))
                .on_click(|_, window, cx| crate::settings::open(window, cx)),
        )
        .child({
            let account = handle.clone();
            let (name, detail) = match shell.account() {
                Some(signed_in) => (signed_in.display().to_owned(), "Steam"),
                None => ("Not signed in".to_owned(), "Sign in to Steam"),
            };
            let who = div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().text_sm().truncate().child(name))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                );
            if shell.account().is_none() {
                row("account", false, cx)
                    .child(account_tile(shell.account(), cx))
                    .child(who)
                    .on_click(move |_, window, cx| Shell::open_account(account.clone(), window, cx))
                    .into_any_element()
            } else {
                // Signed in: the same row opens a menu beside it, like other launchers' account menus.
                Button::new("account")
                    .ghost()
                    .w_full()
                    .h_auto()
                    // 8 px like `row`, plus the 1 px border the other rows have and this button hasn't.
                    .p(px(9.))
                    // The button centres its content; a full-width child keeps it at the left.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .min_w_0()
                            .child(account_tile(shell.account(), cx))
                            .child(who),
                    )
                    .dropdown_menu_with_anchor(Anchor::LeftCenter, move |menu, _, _| {
                        let account = account.clone();
                        menu.item(
                            PopupMenuItem::new("Sign out")
                                .icon(IconName::LogOut)
                                .on_click(move |_, _, cx| Shell::sign_out(account.clone(), cx)),
                        )
                    })
                    .into_any_element()
            }
        })
}
