//! Sign-in dialog: a QR code for the Steam mobile app, or account name and password.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use steam::auth::Confirmation;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use super::{Account, Event, Start};
use crate::ui::{FancyButton, Loader};

const MODULE: Pixels = px(5.);

type Done = Rc<dyn Fn(Account, &mut Window, &mut App)>;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Qr,
    Password,
}

enum State {
    Starting,
    /// QR modules, row by row (`true` = dark), and the side length.
    Qr(Vec<bool>, usize),
    Form,
    Code(Confirmation),
    Waiting(&'static str),
    Failed(String),
}

pub fn open(
    done: impl Fn(Account, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let done: Done = Rc::new(done);
    let view = cx.new(|cx| {
        let mut login = Login::new(done, window, cx);
        login.start(Start::Qr, cx);
        login
    });
    window.open_dialog(cx, move |dialog, _, _| {
        // Fades in settling from just above, and back out (the default drops from the window top).
        let dialog = dialog.entrance(DialogEntrance::FadeSlide);
        dialog
            .title("Sign in to Steam")
            .w(px(420.))
            .child(view.clone())
    });
}

struct Login {
    done: Done,
    /// The window the dialog lives in. Finishing a login must not depend on which window has
    /// focus: the user is usually on their phone when the QR login completes.
    window: AnyWindowHandle,
    mode: Mode,
    state: State,
    account: Entity<InputState>,
    password: Entity<InputState>,
    code: Entity<InputState>,
    /// Sends Steam Guard codes to the running login; dropping it cancels the login.
    codes: Option<UnboundedSender<String>>,
    task: Option<Task<()>>,
}

impl Login {
    fn new(done: Done, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            done,
            window: window.window_handle(),
            mode: Mode::Qr,
            state: State::Starting,
            account: cx.new(|cx| InputState::new(window, cx).placeholder("Account name")),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Password")
                    .masked(true)
            }),
            code: cx.new(|cx| InputState::new(window, cx).placeholder("Code")),
            codes: None,
            task: None,
        }
    }

    fn start(&mut self, start: Start, cx: &mut Context<Self>) {
        let (events_tx, mut events) = unbounded_channel();
        let (codes_tx, codes) = unbounded_channel();
        super::run(start, events_tx, codes);
        self.codes = Some(codes_tx);
        self.state = State::Starting;
        // Replacing the task drops the old receiver, which stops the old login.
        self.task = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = events.recv().await {
                let keep = this.update(cx, |this, cx| this.event(event, cx)).is_ok();
                if !keep {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        self.state = match event {
            Event::Qr(url) => match qrcode::QrCode::new(url.as_bytes()) {
                Ok(code) => {
                    let dark = code
                        .to_colors()
                        .iter()
                        .map(|c| *c == qrcode::Color::Dark)
                        .collect();
                    State::Qr(dark, code.width())
                }
                Err(err) => State::Failed(err.to_string()),
            },
            Event::Code(kind) => State::Code(kind),
            Event::Waiting(text) => State::Waiting(text),
            Event::Failed(err) => State::Failed(err),
            Event::Done(account) => {
                let (done, handle) = (self.done.clone(), self.window);
                cx.spawn(async move |_, cx| {
                    let saved = super::save(&account).await;
                    let _ = handle.update(cx, |_, window, cx| {
                        let note = match saved {
                            Ok(false) => None,
                            Ok(true) => Some("No system keyring found, so your login is saved in a file only your user can read".to_owned()),
                            Err(err) => Some(format!("Signed in, but lulo couldn't remember it for next time: {err}")),
                        };
                        if let Some(note) = note {
                            window.push_notification(note, cx);
                        }
                        window.close_dialog(cx);
                        done(account, window, cx);
                    });
                })
                .detach();
                State::Waiting("Signing in")
            }
        };
        cx.notify();
    }

    fn switch(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.task = None;
        self.codes = None;
        match mode {
            Mode::Qr => self.start(Start::Qr, cx),
            Mode::Password => {
                self.state = State::Form;
                cx.notify();
            }
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        match &self.state {
            State::Form => {
                let account = self.account.read(cx).value().trim().to_owned();
                let password = self.password.read(cx).value().to_string();
                if !account.is_empty() && !password.is_empty() {
                    self.start(Start::Password { account, password }, cx);
                }
            }
            State::Code(_) => {
                let code = self.code.read(cx).value().trim().to_owned();
                if let (false, Some(codes)) = (code.is_empty(), &self.codes) {
                    let _ = codes.send(code);
                    self.state = State::Waiting("Checking the code");
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        match self.mode {
            Mode::Qr => self.start(Start::Qr, cx),
            Mode::Password => {
                self.state = State::Form;
                cx.notify();
            }
        }
    }

    fn tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |mode: Mode, label: &'static str, cx: &mut Context<Self>| {
            let active = self.mode == mode;
            let theme = cx.theme();
            div()
                .id(label)
                .flex_1()
                .py_1p5()
                .rounded(theme.radius)
                .text_center()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .map(|this| {
                    if active {
                        this.bg(theme.group_box)
                            .border_1()
                            .border_color(theme.border)
                    } else {
                        this.text_color(theme.muted_foreground)
                    }
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.switch(mode, cx)))
        };
        let qr = tab(Mode::Qr, "QR code", cx);
        let password = tab(Mode::Password, "Password", cx);
        let theme = cx.theme();
        div()
            .flex()
            .gap_1()
            .p_1()
            .rounded(theme.radius_lg)
            .bg(theme.muted)
            .child(qr)
            .child(password)
    }

    fn body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let note = |text: String| div().text_sm().text_color(muted).text_center().child(text);
        match &self.state {
            State::Starting => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(Loader)
                .child(note("Connecting to Steam".into()))
                .into_any_element(),
            State::Qr(dark, width) => {
                // A light quiet zone around the code keeps it scannable in the dark theme too.
                let rows = dark.chunks(*width).map(|row| {
                    div().flex().children(
                        row.iter()
                            .map(|&on| div().size(MODULE).when(on, |m| m.bg(gpui_kit::black()))),
                    )
                });
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .p(MODULE * 3.)
                            .rounded(px(12.))
                            .bg(white())
                            .children(rows),
                    )
                    .child(note("Scan it with the Steam mobile app to sign in.".into()))
                    .into_any_element()
            }
            State::Form => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Input::new(&self.account))
                .child(Input::new(&self.password))
                .child(note(
                    "Your password goes straight to Steam; lulo keeps only the sign in token."
                        .into(),
                ))
                .into_any_element(),
            State::Code(kind) => {
                let text = match kind {
                    Confirmation::EmailCode => "Enter the code Steam sent to your email.",
                    _ => "Enter the code from your Steam mobile authenticator.",
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(note(text.into()))
                    .child(Input::new(&self.code))
                    .into_any_element()
            }
            State::Waiting(text) => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(Loader)
                .child(note((*text).into()))
                .into_any_element(),
            State::Failed(err) => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .size_6()
                        .text_color(muted),
                )
                .child(note(format!("Couldn't sign in: {err}")))
                .child(
                    Button::new("retry")
                        .outline()
                        .cursor_pointer()
                        .label("Try again")
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                )
                .into_any_element(),
        }
    }
}

impl Render for Login {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let action = match self.state {
            State::Form => Some("Sign in"),
            State::Code(_) => Some("Continue"),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.tabs(cx))
            .child(div().min_h(px(120.)).child(self.body(cx)))
            .children(action.map(|label| {
                div().flex().justify_end().child(
                    FancyButton::new("submit", label)
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                )
            }))
    }
}
