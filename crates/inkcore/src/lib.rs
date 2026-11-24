pub mod app;
pub mod clock;
pub mod config;
pub(crate) mod ctx;
mod dialog;
pub mod draw;
mod emu;
pub mod fixtures;
pub mod input;
mod lockscreen;
pub mod log;
pub mod lua;

pub use crate::app::AppFiles;
pub use crate::config::{
    Battery, Config, NetMode, Persisted, APP_FILE_SAVE_MAX, STORAGE_QUOTA, STORAGE_WRITE_MAX,
};
pub use crate::ctx::files::{app_file_key, app_file_name_ok, storage_name_ok};
pub use crate::ctx::net::{Fetched, Fetcher, NetNote, NetResult, OutRequest, DEFAULT_DELAY_MS};
pub use crate::draw::{Frame, TextItem};
pub use crate::emu::{Core, Pending, Status, ERROR_TITLE};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
