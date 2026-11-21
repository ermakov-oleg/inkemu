pub mod app;
pub mod clock;
pub mod config;
pub(crate) mod ctx;
mod dialog;
pub mod draw;
mod emu;
pub mod fixtures;
pub mod input;
pub mod log;
pub mod lua;

pub use crate::app::AppFiles;
pub use crate::config::{Battery, Config, NetMode, Persisted, STORAGE_QUOTA};
pub use crate::ctx::files::valid_name;
pub use crate::ctx::net::{NetResult, OutRequest, DEFAULT_DELAY_MS};
pub use crate::draw::{Frame, TextItem};
pub use crate::emu::{Core, Pending, Status, ERROR_TITLE, NOT_YET};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
