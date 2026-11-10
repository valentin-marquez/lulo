//! Home page: jump back into recent instances, browse the whole library.

#[path = "library.view.rs"]
mod library;
#[path = "play.view.rs"]
mod play;

use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

use crate::instance::Instance;
use library::Library;
use play::Play;

const RECENT: usize = 4;

pub struct Home {
    instances: Vec<Instance>,
    search: Entity<InputState>,
}

impl Home {
    pub fn new(instances: Vec<Instance>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            instances,
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search instances")),
        }
    }

    /// Instances played at least once, most recent first.
    pub fn recent(&self) -> Vec<Instance> {
        self.instances
            .iter()
            .filter(|i| i.last_played.is_some())
            .take(RECENT)
            .cloned()
            .collect()
    }
}

impl Render for Home {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_lowercase();
        let shown = self
            .instances
            .iter()
            .filter(|i| i.name.to_lowercase().contains(&query))
            .cloned()
            .collect();

        div().id("home").size_full().overflow_y_scrollbar().child(
            div()
                .flex()
                .flex_col()
                .gap_8()
                .p_6()
                .child(Play::new(self.recent()))
                .child(Library::new(shown, self.search.clone())),
        )
    }
}
