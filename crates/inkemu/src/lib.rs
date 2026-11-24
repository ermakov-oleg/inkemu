//! The native shell around `inkcore`: the app folder, the state directory on disk and the 0.1 defaults.
mod net_live;
mod paths;
mod secrets;
mod session;
mod state_disk;

pub use session::{Session, SessionOptions};
