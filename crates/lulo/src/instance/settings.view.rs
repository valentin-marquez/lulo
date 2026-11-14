//! "Instance settings" dialog, in sections: name and deleting, how the game starts, and the
//! options most people never touch.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::picture::{self, Picker};
use super::{Icon, Instance, store};
use crate::settings::{NAV, WIDTH, heading};
use crate::shell::Shell;
use crate::ui::{self, FancyButton};

/// Taller than the app's settings: General and Game hold more than fits in its height, and a
/// form that scrolls without saying so hides its last fields.
const HEIGHT: f32 = 560.;
/// Height shared by the buttons of the form.
const CONTROL: Pixels = px(36.);
/// What the game gives itself when nothing is set (`-Xmx3072m` in its launcher config).
const DEFAULT_MEMORY: u32 = 3;
/// Less than this and the game struggles to load a world.
const MIN_MEMORY: u32 = 2;

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    General,
    Customize,
    Game,
}

impl Section {
    const ALL: [Section; 3] = [Section::General, Section::Customize, Section::Game];

    fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Customize => "Customize",
            Section::Game => "Game",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Section::General => IconName::Settings,
            Section::Customize => IconName::Palette,
            Section::Game => IconName::Rocket,
        }
    }
}

/// Opens the dialog on `section`.
pub fn open(
    instance: Instance,
    section: Section,
    shell: WeakEntity<Shell>,
    window: &mut Window,
    cx: &mut App,
) {
    let view = cx.new(|cx| Settings::new(instance, section, shell, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        // Fades in settling from just above, and back out (the default drops from the window top).
        let dialog = dialog.entrance(DialogEntrance::FadeSlide);
        dialog
            .title("Instance settings")
            .centered()
            .w(px(WIDTH))
            .child(view.clone())
    });
}

struct Settings {
    instance: Instance,
    shell: WeakEntity<Shell>,
    section: Section,
    picture: Entity<Picker>,
    name: Entity<InputState>,
    /// `None` leaves the game's own default; moving the slider sets it.
    memory_gb: Option<u32>,
    slider: Entity<SliderState>,
    /// This computer's memory in GB, the slider's upper end.
    installed: u32,
    jvm_args: Entity<InputState>,
    launch_args: Entity<InputState>,
    /// The section's scroll position, for the scrollbar that shows when a small window
    /// still cuts the form short.
    scroll: ScrollHandle,
    _slider_moved: Subscription,
    /// Redraw as the fields and the picture change, so the footer can say there is
    /// something to save.
    _edits: Vec<Subscription>,
}

/// This computer's memory in whole GB.
fn installed_memory() -> u32 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    (system.total_memory() >> 30) as u32
}

/// The slider's upper end: all the memory there is, and never below what is already set.
fn memory_limit(installed: u32, set: Option<u32>) -> u32 {
    installed.max(set.unwrap_or(0)).max(DEFAULT_MEMORY + 1)
}

fn title(text: &'static str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn field(name: &'static str, input: &Entity<InputState>, line: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1p5()
        .child(title(name))
        .child(Input::new(input))
        .child(hint(line, cx))
}

impl Settings {
    fn new(
        instance: Instance,
        section: Section,
        shell: WeakEntity<Shell>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut input = |value: String, placeholder: &'static str| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let name = input(instance.name.to_string(), "Name");
        let jvm_args = input(instance.jvm_args.clone(), "-XX:+AlwaysPreTouch");
        let launch_args = input(instance.launch_args.clone(), "-debug");
        // Without a chosen icon the instance shows the one picked from its id; start from that.
        let shown = instance
            .icon
            .clone()
            .unwrap_or_else(|| Icon::picked(&instance.id));
        let picture = cx.new(|_| Picker::new(Some(shown), true));
        let installed = installed_memory();
        let slider = cx.new(|_| {
            SliderState::new()
                .min(MIN_MEMORY as f32)
                .max(memory_limit(installed, instance.memory_gb) as f32)
                .step(1.)
                .default_value(instance.memory_gb.unwrap_or(DEFAULT_MEMORY) as f32)
        });
        let moved = cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            this.memory_gb = Some(value.start().round() as u32);
            cx.notify();
        });
        let mut edits: Vec<Subscription> = [&name, &jvm_args, &launch_args]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                    if let InputEvent::Change = event {
                        cx.notify();
                    }
                })
            })
            .collect();
        edits.push(cx.observe(&picture, |_, _, cx| cx.notify()));
        Self {
            _edits: edits,
            scroll: ScrollHandle::new(),
            memory_gb: instance.memory_gb,
            picture,
            name,
            slider,
            installed,
            jvm_args,
            launch_args,
            instance,
            shell,
            section,
            _slider_moved: moved,
        }
    }

    /// Whether anything shown differs from what the instance has saved.
    fn dirty(&self, cx: &App) -> bool {
        let text = |input: &Entity<InputState>| input.read(cx).value();
        let instance = &self.instance;
        text(&self.name).trim() != instance.name.as_ref()
            || self.memory_gb != instance.memory_gb
            || text(&self.jvm_args).trim() != instance.jvm_args
            || text(&self.launch_args).trim() != instance.launch_args
            || self.picture.read(cx).icon != instance.icon()
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let settings = store::Settings {
            name: text(&self.name),
            memory_gb: self.memory_gb,
            jvm_args: text(&self.jvm_args),
            launch_args: text(&self.launch_args),
        };
        let dir = &self.instance.dir;
        // An uploaded picture is copied into the instance; one already there is kept as is.
        let icon = self.picture.read(cx).icon.clone();
        let saved = store::configure(dir, &settings).and_then(|()| {
            if self.instance.icon.as_ref() == Some(&icon) {
                return Ok(());
            }
            store::set_icon(dir, Some(picture::keep(&icon, dir)?))
        });
        match saved {
            Ok(()) => {
                window.close_dialog(cx);
                let _ = self.shell.update(cx, |shell, cx| shell.reload(cx));
            }
            Err(err) => window
                .push_notification(ui::toast::failed_because("Hmm, that didn't save", err), cx),
        }
    }

    /// Memory as a slider up to what this computer has; untouched, the game keeps its default.
    fn memory(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let value = match self.memory_gb {
            Some(gb) => format!("{gb} GB"),
            None => format!("Game default, {DEFAULT_MEMORY} GB"),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    // Fixed, so the Default button showing up doesn't push the slider down.
                    .h(CONTROL)
                    .child(title("Memory"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(self.memory_gb.is_none(), |this| {
                                        this.text_color(theme.muted_foreground)
                                    })
                                    .child(value),
                            )
                            .when(self.memory_gb.is_some(), |this| {
                                this.child(
                                    Button::new("memory-reset")
                                        .ghost()
                                        .cursor_pointer()
                                        .icon(IconName::Undo2)
                                        .label("Default")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.memory_gb = None;
                                            this.slider.update(cx, |slider, cx| {
                                                slider.set_value(DEFAULT_MEMORY as f32, window, cx)
                                            });
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            .child(Slider::new(&self.slider).cursor_pointer())
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(hint(format!("{MIN_MEMORY} GB"), cx))
                    .child(hint(format!("This computer has {} GB", self.installed), cx)),
            )
    }

    /// What the instance runs and where it lives, with a way to its folder.
    fn about(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let instance = &self.instance;
        let build = match instance.build {
            Some(build) => format!("Build {build}"),
            None => "Not installed yet".to_owned(),
        };
        let fact = |name: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(name),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(value),
                )
        };
        let dir = instance.dir.clone();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .rounded(theme.radius_lg)
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .gap_8()
                    .child(fact("Branch", instance.branch.to_string()))
                    .child(fact("Game", build))
                    .child(fact("Activity", instance.played())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(fact("Folder", instance.dir.display().to_string()).flex_1())
                    .child(
                        Button::new("open-folder")
                            .ghost()
                            .cursor_pointer()
                            .icon(IconName::FolderOpen)
                            .label("Open")
                            .on_click(move |_, _, cx| cx.open_with_system(&dir)),
                    ),
            )
    }

    /// Deleting asks in the same dialog the Library's menu opens.
    fn danger(&self, cx: &mut Context<Self>) -> Div {
        let (shell, id) = (self.shell.clone(), self.instance.id.clone());
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .pt_5()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .min_w_0()
                    .child(title("Delete instance"))
                    .child(hint("Removes the instance with its saves and mods.", cx)),
            )
            .child(
                FancyButton::new("delete", "Delete")
                    .small()
                    .danger()
                    .icon(IconName::Trash)
                    .on_click(move |_, window, cx| {
                        Shell::confirm_delete(shell.clone(), id.clone(), window, cx)
                    }),
            )
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let fields = div().flex().flex_col().gap_5();
        let (line, fields) = match self.section {
            Section::General => (
                "What this instance is called and what it runs.",
                fields
                    // A name is a few words: a field the width of the dialog looked lost.
                    .child(
                        field(
                            "Name",
                            &self.name,
                            "Renaming keeps the folder on disk as it is.",
                            cx,
                        )
                        .max_w(px(380.)),
                    )
                    .child(self.about(cx))
                    .child(self.danger(cx)),
            ),
            Section::Customize => (
                "The picture this instance shows in the Library.",
                fields.child(self.picture.clone()),
            ),
            Section::Game => (
                "How the game starts in this instance.",
                fields
                    .child(self.memory(cx))
                    // The two argument fields are for the few who need them, so they sit
                    // under their own heading and not in a section of one field each.
                    .child(
                        div()
                            .pt_4()
                            .border_t_1()
                            .border_color(border)
                            .child(title("Advanced")),
                    )
                    .child(field(
                        "Launch arguments",
                        &self.launch_args,
                        "Options for the game, like -debug or -nosteam.",
                        cx,
                    ))
                    .child(field(
                        "Java arguments",
                        &self.jvm_args,
                        "Options for the Java VM the game runs on. A -Xmx here wins over Memory.",
                        cx,
                    )),
            ),
        };
        let content = div()
            .flex()
            .flex_col()
            .gap_5()
            .child(heading(self.section.label(), line, cx))
            .child(fields);
        let nav = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .w(px(NAV))
            .p_3()
            .border_r_1()
            .border_color(border)
            .children(Section::ALL.map(|section| {
                crate::ui::nav_item(section.label(), section.icon(), section == self.section, cx)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    }))
            }));
        let footer = div()
            .flex()
            .flex_none()
            .gap_2()
            .px_6()
            .py_3()
            .border_t_1()
            .border_color(border)
            .items_center()
            // Save keeps every section at once; this says so when one of them changed.
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .children(self.dirty(cx).then_some("You have unsaved changes")),
            )
            .child(
                ui::secondary("cancel", "Cancel").on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                FancyButton::new("save", "Save")
                    .small()
                    .icon(IconName::Check)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            );
        div()
            .flex()
            .h(px(HEIGHT))
            .border_t_1()
            .border_color(border)
            .child(nav)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id("instance-settings-content")
                                    .size_full()
                                    .p_6()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .child(ui::enter(content, self.section.label())),
                            )
                            .vertical_scrollbar(&self.scroll),
                    )
                    .child(footer),
            )
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: the gpui glob brings its own `test` attribute, which would shadow Rust's.
    use super::{DEFAULT_MEMORY, installed_memory, memory_limit};

    #[test]
    fn the_slider_reaches_the_installed_memory_and_what_is_set() {
        assert_eq!(memory_limit(32, None), 32);
        assert_eq!(memory_limit(32, Some(8)), 32);
        // Set on a bigger computer, or by hand: still reachable.
        assert_eq!(memory_limit(16, Some(24)), 24);
        // Memory couldn't be read: the slider still has a range.
        assert_eq!(memory_limit(0, None), DEFAULT_MEMORY + 1);
        assert!(installed_memory() > 0);
    }
}
