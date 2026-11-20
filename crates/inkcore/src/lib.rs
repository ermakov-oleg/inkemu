pub mod app;
pub mod clock;
pub mod config;
mod emu;
pub mod log;
pub mod lua;

pub use crate::app::AppFiles;
pub use crate::config::{Battery, Config, NetMode, Persisted};
pub use crate::emu::{Core, Frame, Pending, Status, TextItem, ERROR_TITLE};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
