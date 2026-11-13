//! Window frame: title bar, navigation and instances on the left, the current page on the right.

#[path = "downloads.view.rs"]
mod downloads;
#[path = "sidebar.view.rs"]
mod sidebar;
#[path = "topbar.view.rs"]
mod topbar;

use std::collections::HashMap;
use std::path::PathBuf;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::account::{self, Account};
use crate::instance::{self, Instance, Status, detail::Detail, install, launch, store};
use crate::ui::Loader;
use crate::{home, screenshots};
use topbar::Topbar;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Screenshots,
    Instance(usize),
}

pub struct Shell {
    root: PathBuf,
    instances: Vec<Instance>,
    page: Page,
    detail: Option<Entity<Detail>>,
    account: Option<Account>,
    /// Newest Steam build id per branch name; empty until the first check succeeds.
    latest: HashMap<SharedString, u32>,
    /// Installs in progress by instance id.
    installs: HashMap<SharedString, install::Download>,
    /// Games running, by instance id.
    running: HashMap<SharedString, std::process::Child>,
    /// Shows the loader under the title bar while the page's data loads.
    // ponytail: listing instances is instant; installs (roadmap 5) will set it.
    loading: bool,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let root = store::root();
        store::migrate(&root);
        cx.spawn(async move |this, cx| {
            let saved = account::load().await;
            let _ = this.update(cx, |shell, cx| {
                shell.account = saved;
                cx.notify();
            });
        })
        .detach();
        let mut shell = Self {
            account: None,
            latest: HashMap::new(),
            installs: HashMap::new(),
            running: HashMap::new(),
            instances: store::list(&root),
            root,
            page: Page::Home,
            detail: None,
            loading: false,
        };
        shell.check_builds(cx);
        shell
    }

    /// Newest build of the instance's branch, when it is known.
    pub fn latest(&self, instance: &Instance) -> Option<u32> {
        self.latest.get(&instance.branch).copied()
    }

    /// Asks Steam anonymously for each branch's build; offline just leaves the marks as they were.
    fn check_builds(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(branches) = instance::branches::fetch().await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| {
                shell.latest = branches
                    .into_iter()
                    .map(|b| (SharedString::from(b.name), b.build_id))
                    .collect();
                if let (Page::Instance(ix), Some(detail)) = (shell.page, &shell.detail)
                    && let Some(instance) = shell.instances.get(ix).cloned()
                {
                    detail.update(cx, |d, cx| d.show(instance, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if let Page::Instance(ix) = page {
            let Some(instance) = self.instances.get(ix).cloned() else {
                return;
            };
            match &self.detail {
                Some(detail) => detail.update(cx, |d, cx| d.show(instance, cx)),
                None => {
                    let shell = cx.entity().downgrade();
                    self.detail = Some(cx.new(|_| Detail::new(instance, shell)));
                }
            }
            self.check_builds(cx);
        }
        self.page = page;
        cx.notify();
    }

    pub fn status(&self, instance: &Instance) -> Status {
        if self.running.contains_key(&instance.id) {
            return Status::Running;
        }
        match self.installs.get(&instance.id) {
            Some(download) => Status::Installing(download.pct()),
            None => instance.status(self.latest(instance)),
        }
    }

    /// The main button of an instance: install it if needed, else launch it; stops whatever
    /// it is doing (a running game, an install).
    pub fn play(shell: WeakEntity<Shell>, id: SharedString, window: &mut Window, cx: &mut App) {
        let Ok((instance, account)) = shell.read_with(cx, |shell, _| {
            let instance = shell.instances.iter().find(|i| i.id == id).cloned();
            (instance, shell.account.clone())
        }) else {
            return;
        };
        let Some(instance) = instance else { return };
        use gpui_kit::component::WindowExt;
        let status = shell
            .read_with(cx, |shell, _| shell.status(&instance))
            .unwrap_or(Status::Ready);
        match (status, account) {
            (Status::Running, _) => {
                let _ = shell.update(cx, |shell, _| {
                    if let Some(child) = shell.running.get_mut(&instance.id) {
                        let _ = child.kill();
                    }
                });
            }
            (Status::Ready | Status::Outdated, _) => match launch::start(&instance) {
                Ok(child) => {
                    let _ = shell.update(cx, |shell, cx| shell.watch(instance, child, cx));
                }
                Err(err) => window.push_notification(format!("Couldn't start the game: {err}"), cx),
            },
            (Status::Installing(_), _) => {
                let _ = shell.update(cx, |shell, cx| shell.cancel(&instance.id, cx));
            }
            (_, None) => Self::open_account(shell, window, cx),
            (_, Some(account)) => {
                let handle = window.window_handle();
                let _ = shell.update(cx, |shell, cx| shell.install(instance, account, handle, cx));
            }
        }
    }

    /// Tracks a running game until it exits, then records the play time.
    fn watch(&mut self, instance: Instance, child: std::process::Child, cx: &mut Context<Self>) {
        let id = instance.id.clone();
        self.running.insert(id.clone(), child);
        self.changed(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let exited = this
                    .update(cx, |shell, _| match shell.running.get_mut(&id) {
                        Some(child) => !matches!(child.try_wait(), Ok(None)),
                        None => true,
                    })
                    .unwrap_or(true);
                if exited {
                    break;
                }
            }
            let _ = store::set_last_played(&instance.dir, store::now());
            let _ = this.update(cx, |shell, cx| {
                shell.running.remove(&id);
                shell.reload(cx);
            });
        })
        .detach();
    }

    /// Name of a running game, for the title bar.
    pub fn running(&self) -> Option<SharedString> {
        let id = self.running.keys().next()?;
        self.instances
            .iter()
            .find(|i| &i.id == id)
            .map(|i| i.name.clone())
    }

    fn install(
        &mut self,
        instance: Instance,
        account: Account,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if self.installs.contains_key(&instance.id) {
            return;
        }
        let id = instance.id.clone();
        let (events_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let name = instance.name.clone();
        let cancel = install::start(account, instance, events_tx);
        self.installs
            .insert(id.clone(), install::Download::new(name, cancel));
        self.changed(cx);
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.recv().await {
                let _ = this.update(cx, |shell, cx| match event {
                    install::Event::Progress { done, total } => {
                        // Gone means cancelled; events already queued must not bring it back.
                        if let Some(download) = shell.installs.get_mut(&id)
                            && download.progress(done, total, std::time::Instant::now())
                        {
                            shell.changed(cx);
                        }
                    }
                    install::Event::Done => {
                        shell.installs.remove(&id);
                        shell.reload(cx);
                    }
                    install::Event::Failed(err) => {
                        if shell.installs.remove(&id).is_none() {
                            return;
                        }
                        shell.changed(cx);
                        // The window that started the install, not whichever has focus now.
                        let _ = window.update(cx, |_, window, cx| {
                            use gpui_kit::component::WindowExt;
                            window.push_notification(format!("Install failed: {err}"), cx);
                        });
                    }
                });
            }
        })
        .detach();
    }

    /// Installs in progress, by name so the order holds still between frames.
    pub fn downloads(&self) -> Vec<(&SharedString, &install::Download)> {
        let mut all: Vec<_> = self.installs.iter().collect();
        all.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        all
    }

    /// Stops an install; what it wrote stays for the next one to reuse.
    pub fn cancel(&mut self, id: &SharedString, cx: &mut Context<Self>) {
        if let Some(download) = self.installs.remove(id) {
            download.cancel();
            self.changed(cx);
        }
    }

    /// Re-reads instances from disk, keeping the current page.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.instances = store::list(&self.root);
        if let (Page::Instance(_), Some(detail)) = (self.page, &self.detail) {
            let shown = detail.read(cx).id();
            if let Some(ix) = self.instances.iter().position(|i| i.id == shown) {
                return self.go(Page::Instance(ix), cx);
            }
        }
        self.changed(cx);
    }

    /// Repaints the shell and the detail view (a child entity doesn't redraw on its own).
    fn changed(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = &self.detail {
            detail.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    /// Signed out: the sign-in dialog. Signed in: who it is, and a way to sign out.
    pub fn open_account(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
        let Ok(current) = shell.read_with(cx, |shell, _| shell.account.clone()) else {
            return;
        };
        match current {
            None => account::login::open(
                move |signed_in, _, cx| {
                    let _ = shell.update(cx, |shell, cx| {
                        shell.account = Some(signed_in);
                        cx.notify();
                    });
                },
                window,
                cx,
            ),
            Some(current) => {
                use gpui_kit::component::WindowExt;
                window.open_dialog(cx, move |dialog, _, _| {
                    let shell = shell.clone();
                    dialog
                        .title("Steam account")
                        .w(px(380.))
                        .child(format!("Signed in as {}.", current.display()))
                        .child(
                            div().flex().justify_end().pt_4().child(
                                crate::ui::FancyButton::new("sign-out", "Sign out")
                                    .small()
                                    .on_click(move |_, window, cx| {
                                        window.close_dialog(cx);
                                        let shell = shell.clone();
                                        cx.spawn(async move |cx| {
                                            let _ = account::forget().await;
                                            let _ = shell.update(cx, |shell, cx| {
                                                shell.account = None;
                                                cx.notify();
                                            });
                                        })
                                        .detach();
                                    }),
                            ),
                        )
                });
            }
        }
    }

    /// Opens the create dialog; a created instance is listed and selected.
    pub fn create(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
        let Ok(root) = shell.read_with(cx, |shell, _| shell.root.clone()) else {
            return;
        };
        instance::create::open(
            root,
            move |created, _, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.instances = store::list(&shell.root);
                    let ix = shell.instances.iter().position(|i| i.id == created.id);
                    shell.go(ix.map_or(Page::Home, Page::Instance), cx);
                });
            },
            window,
            cx,
        );
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match (self.page, &self.detail) {
            (Page::Instance(_), Some(detail)) => detail.clone().into_any_element(),
            (Page::Screenshots, _) => screenshots::render().into_any_element(),
            _ => home::render(self, cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().title_bar)
            .text_color(cx.theme().foreground)
            .child(Topbar {
                running: self.running(),
                downloads: downloads::Pill::new(self, cx.entity().downgrade()),
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar::render(self, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .rounded_tl(px(14.))
                            .border_t_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .overflow_hidden()
                            .when(self.loading, |this| this.child(Loader))
                            .child(div().flex_1().min_h_0().child(page)),
                    ),
            )
    }
}
