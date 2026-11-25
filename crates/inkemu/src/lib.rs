//! The native shell around `inkcore`: the app folder, the state directory on disk and the 0.1 defaults.
pub mod cli;
pub mod command;
pub mod daemon;
pub mod mcp;
mod net_live;
mod paths;
mod secrets;
mod session;
mod state_disk;
pub mod steps;

pub use session::{Session, SessionOptions};
