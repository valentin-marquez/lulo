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
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::account::{self, Account};
use crate::instance::{self, Instance, Status, detail::Detail, install, launch, store};
use crate::library::Library;
use crate::screenshots;
use crate::ui::Loader;
use topbar::Topbar;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Library,
    Screenshots,
    Instance(usize),
}

pub struct Shell {
    root: PathBuf,
    instances: Vec<Instance>,
    page: Page,
    detail: Option<Entity<Detail>>,
    /// Made on first view; it needs a window for its search field.
    library: Option<Entity<Library>>,
    account: Option<Account>,
    /// Newest Steam build id per branch name; empty until the first check succeeds.
    latest: HashMap<SharedString, u32>,
    /// Downloads in progress: a game install by its instance id, a mod by [`mod_key`].
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
            page: Page::Library,
            detail: None,
            library: None,
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
            // The game quits silently without Steam, so say so and offer to open it.
            (Status::Ready | Status::Outdated, _) if !launch::steam_running() => {
                use gpui_kit::component::WindowExt;
                window.push_notification(crate::ui::toast::steam_closed(), cx);
            }
            (Status::Ready | Status::Outdated, _) => match launch::start(&instance) {
                Ok(child) => {
                    let _ = shell.update(cx, |shell, cx| shell.watch(instance, child, cx));
                }
                Err(err) => {
                    use gpui_kit::component::WindowExt;
                    window.push_notification(
                        crate::ui::toast::failed_because("The game refused to start", err),
                        cx,
                    )
                }
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

    /// Asks before updating an outdated instance: a new build can break saves and mods.
    pub fn confirm_update(
        shell: WeakEntity<Shell>,
        id: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        use gpui_kit::component::WindowExt;
        let Ok((instance, account)) = shell.read_with(cx, |shell, _| {
            let instance = shell.instances.iter().find(|i| i.id == id).cloned();
            (
                instance.map(|i| (shell.latest(&i), i)),
                shell.account.clone(),
            )
        }) else {
            return;
        };
        let Some((Some(newest), instance)) = instance else {
            return;
        };
        let Some(have) = instance.build else { return };
        let Some(account) = account else {
            return Self::open_account(shell, window, cx);
        };
        let text = format!(
            "Build {have} will be replaced by build {newest}. A new build can break saves and \
             mods. Files that didn't change are not downloaded again."
        );
        window.open_dialog(cx, move |dialog, _, _| {
            // Fades in settling from just above, and back out (the default drops from the window top).
            let dialog = dialog.entrance(DialogEntrance::FadeSlide);
            let (shell, instance, account) = (shell.clone(), instance.clone(), account.clone());
            dialog
                .title(format!("Update {}?", instance.name))
                .w(px(420.))
                .child(text.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .pt_4()
                        .child(
                            crate::ui::secondary("cancel-update", "Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            crate::ui::FancyButton::new("confirm-update", "Update")
                                .small()
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let handle = window.window_handle();
                                    let _ = shell.update(cx, |shell, cx| {
                                        shell.install(instance.clone(), account.clone(), handle, cx)
                                    });
                                }),
                        ),
                )
        });
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
                            window.push_notification(
                                crate::ui::toast::failed_because("Yikes, the install tripped", err),
                                cx,
                            );
                        });
                    }
                });
            }
        })
        .detach();
    }

    /// Installs in progress, by name so the order holds still between frames.
    /// Whether a mod is being downloaded into instance `id`.
    pub fn adding_mods(&self, id: &SharedString) -> bool {
        let prefix = mod_key(id, "");
        self.installs.keys().any(|key| key.starts_with(&*prefix))
    }

    /// Downloads Workshop item `item` into `instance`, shown in the download manager like a
    /// game install: progress, speed, cancel.
    pub fn add_mod(
        &mut self,
        instance: Instance,
        item: u64,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        use gpui_kit::component::WindowExt;
        let key = mod_key(&instance.id, &item.to_string());
        if self.installs.contains_key(&key) {
            return;
        }
        let (events_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let task = crate::runtime::spawn(instance::mods::install(
            instance,
            item,
            move |title, done, total| {
                let _ = events_tx.send((SharedString::from(title.to_owned()), done, total));
            },
        ));
        let download = install::Download::new("Workshop mod".into(), task.abort_handle());
        self.installs.insert(key.clone(), download);
        self.changed(cx);
        cx.spawn(async move |this, cx| {
            // The sender lives in the task: this loop ends when the task does.
            while let Some((title, done, total)) = events.recv().await {
                let _ = this.update(cx, |shell, cx| {
                    let Some(download) = shell.installs.get_mut(&key) else {
                        return;
                    };
                    let renamed = download.name != title;
                    download.name = title;
                    if download.progress(done, total, std::time::Instant::now()) || renamed {
                        shell.changed(cx);
                    }
                });
            }
            let added = task.await;
            let _ = this.update(cx, |shell, cx| {
                shell.installs.remove(&key);
                shell.changed(cx);
            });
            let note = match added {
                Ok(Ok(added)) => crate::ui::toast::done(match added.with.len() {
                    0 => format!("{} is in! Ready for your next run", added.title),
                    1 => format!("{} is in, with the mod it needs", added.title),
                    many => format!("{} is in, with the {many} mods it needs", added.title),
                }),
                Ok(Err(why)) => crate::ui::toast::failed_because("That mod didn't make it", why),
                // Cancelled from the download manager: nothing to say.
                Err(_) => return,
            };
            let _ = window.update(cx, |_, window, cx| window.push_notification(note, cx));
        })
        .detach();
    }

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

    /// Shows the page of instance `id`.
    pub fn view(shell: WeakEntity<Shell>, id: SharedString, _: &mut Window, cx: &mut App) {
        let _ = shell.update(cx, |shell, cx| {
            if let Some(ix) = shell.instances.iter().position(|i| i.id == id) {
                shell.go(Page::Instance(ix), cx);
            }
        });
    }

    /// Opens the settings dialog of instance `id`.
    pub fn settings(shell: WeakEntity<Shell>, id: SharedString, window: &mut Window, cx: &mut App) {
        Self::settings_at(shell, id, instance::settings::Section::General, window, cx);
    }

    /// Opens the settings dialog of instance `id` on its picture.
    pub fn customize(
        shell: WeakEntity<Shell>,
        id: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        Self::settings_at(
            shell,
            id,
            instance::settings::Section::Customize,
            window,
            cx,
        );
    }

    fn settings_at(
        shell: WeakEntity<Shell>,
        id: SharedString,
        section: instance::settings::Section,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Ok(Some(instance)) = shell.read_with(cx, |shell, _| {
            shell.instances.iter().find(|i| i.id == id).cloned()
        }) else {
            return;
        };
        instance::settings::open(instance, section, shell, window, cx);
    }

    /// Asks before deleting instance `id`: its saves and mods go with it.
    pub fn confirm_delete(
        shell: WeakEntity<Shell>,
        id: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        use gpui_kit::component::WindowExt;
        let Ok(Some(name)) = shell.read_with(cx, |shell, _| {
            let instance = shell.instances.iter().find(|i| i.id == id);
            instance.map(|i| i.name.clone())
        }) else {
            return;
        };
        window.open_dialog(cx, move |dialog, _, _| {
            // Fades in settling from just above, and back out (the default drops from the window top).
            let dialog = dialog.entrance(DialogEntrance::FadeSlide);
            let (shell, id) = (shell.clone(), id.clone());
            dialog
                .title(format!("Delete {name}?"))
                .w(px(420.))
                .child("Its saves, mods and settings are deleted for good.")
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .pt_4()
                        .child(
                            crate::ui::secondary("cancel-delete", "Keep")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            crate::ui::FancyButton::new("confirm-delete", "Delete")
                                .small()
                                .danger()
                                .icon(gpui_kit::assets::IconName::Trash)
                                .on_click(move |_, window, cx| {
                                    // The instance's settings dialog may be open under this one.
                                    window.close_all_dialogs(cx);
                                    if let Ok(Err(why)) =
                                        shell.update(cx, |shell, cx| shell.remove(&id, window, cx))
                                    {
                                        window.push_notification(crate::ui::toast::failed(why), cx);
                                    }
                                }),
                        ),
                )
        });
    }

    /// Deletes instance `id` with its saves and mods, and goes back to the Library. Refused
    /// while the game runs or an install writes into it. The instance leaves the list at once;
    /// its files (gigabytes, with an own game copy) go in the background.
    pub fn remove(
        &mut self,
        id: &SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        use gpui_kit::component::WindowExt;
        if self.running.contains_key(id) || self.installs.contains_key(id) || self.adding_mods(id) {
            return Err("Whoa! Stop the game before deleting it".into());
        }
        let Some(ix) = self.instances.iter().position(|i| i.id == *id) else {
            return Ok(());
        };
        let instance = self.instances.remove(ix);
        self.page = Page::Library;
        self.changed(cx);
        let name = instance.name.clone();
        window.push_notification(
            crate::ui::toast::working(format!("Sweeping away {name}")),
            cx,
        );
        let (root, handle) = (self.root.clone(), window.window_handle());
        cx.spawn(async move |this, cx| {
            let deleted = crate::runtime::blocking(move || {
                store::delete(&root, &instance.dir).map_err(|err| err.to_string())
            })
            .await
            .and_then(|result| result);
            // Whatever happened, show what is on disk now.
            let _ = this.update(cx, |shell, cx| shell.reload(cx));
            let toast = match deleted {
                Ok(()) => crate::ui::toast::done(format!("Poof! {name} is gone")),
                Err(err) => crate::ui::toast::failed_because("That instance wouldn't budge", err),
            };
            let _ = handle.update(cx, |_, window, cx| window.push_notification(toast, cx));
        })
        .detach();
        Ok(())
    }

    /// Re-reads instances from disk, keeping the current page.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
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
            detail.update(cx, |detail, cx| detail.changed(cx));
        }
        if let Some(library) = &self.library {
            library.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    /// The sign-in dialog, unless an account is already signed in.
    pub fn open_account(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
        if shell
            .read_with(cx, |shell, _| shell.account.is_some())
            .unwrap_or(true)
        {
            return;
        }
        account::login::open(
            move |signed_in, _, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.account = Some(signed_in);
                    cx.notify();
                });
            },
            window,
            cx,
        )
    }

    /// Forgets the saved login.
    pub fn sign_out(shell: WeakEntity<Shell>, cx: &mut App) {
        cx.spawn(async move |cx| {
            let _ = account::forget().await;
            let _ = shell.update(cx, |shell, cx| {
                shell.account = None;
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the create dialog; a created instance is listed, selected and installed.
    pub fn create(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
        let Ok(root) = shell.read_with(cx, |shell, _| shell.root.clone()) else {
            return;
        };
        instance::create::open(
            root,
            move |created, window, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.instances = store::list(&shell.root);
                    let ix = shell.instances.iter().position(|i| i.id == created.id);
                    shell.go(ix.map_or(Page::Library, Page::Instance), cx);
                });
                // A new instance has nothing to run yet: start its install right away, as its
                // main button would (it asks to sign in to Steam first when nobody is).
                Shell::play(shell.clone(), created.id.clone(), window, cx);
            },
            window,
            cx,
        );
    }
}

/// The download manager's key for Workshop item `item` going into instance `instance`.
fn mod_key(instance: &SharedString, item: &str) -> SharedString {
    format!("mod:{instance}:{item}").into()
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match (self.page, &self.detail) {
            (Page::Instance(_), Some(detail)) => detail.clone().into_any_element(),
            (Page::Screenshots, _) => screenshots::render().into_any_element(),
            _ => {
                let shell = cx.entity().downgrade();
                self.library
                    .get_or_insert_with(|| cx.new(|cx| Library::new(shell, window, cx)))
                    .clone()
                    .into_any_element()
            }
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
