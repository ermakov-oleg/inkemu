pub mod app;
pub mod clock;
pub mod config;
pub(crate) mod ctx;
mod emu;
pub mod log;
pub mod lua;

pub use crate::app::AppFiles;
pub use crate::config::{Battery, Config, NetMode, Persisted};
pub use crate::emu::{Core, Frame, Pending, Status, TextItem, ERROR_TITLE, NOT_YET};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
