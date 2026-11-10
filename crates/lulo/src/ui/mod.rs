//! Shared building blocks used across features.

#[path = "empty.component.rs"]
mod empty;
#[path = "fancy-button.component.rs"]
mod fancy_button;
#[path = "loader.component.rs"]
mod loader;
#[path = "meta.component.rs"]
mod meta;

use std::time::Duration;

use gpui_kit::base::Spring;

pub use empty::Empty;
pub use fancy_button::FancyButton;
pub use loader::Loader;
pub use meta::Meta;

/// Default motion for anything that reacts to the user: critically damped, no overshoot.
/// Interactions animate with springs, never with linear timing.
pub const SPRING: Spring = Spring::new(Duration::from_millis(300));
