//! Terminal dashboard. The model and sanitizer always build. The ratatui view, the terminal
//! guard and the event loop need the `tui` feature.

pub mod model;
pub mod sanitize;

#[cfg(feature = "tui")]
pub mod render;
#[cfg(feature = "tui")]
pub mod run;
#[cfg(feature = "tui")]
pub mod terminal;
