use std::collections::BTreeMap;

/// `ctx.storage` quota as 0.1: total bytes and number of files.
pub const STORAGE_QUOTA: (u64, usize) = (134_217_728, 512);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetMode {
    Replay,
    Record,
    Live,
}

impl NetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            NetMode::Replay => "replay",
            NetMode::Record => "record",
            NetMode::Live => "live",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Battery {
    pub percent: i32,
    pub charging: bool,
    pub voltage_mv: i32,
    pub temperature_c: i32,
}

impl Default for Battery {
    fn default() -> Self {
        Battery {
            percent: 87,
            charging: false,
            voltage_mv: 4020,
            temperature_c: 24,
        }
    }
}

/// Everything the host decides about one emulator run.
#[derive(Clone, Debug)]
pub struct Config {
    /// Local wall-clock time at start: milliseconds since 1970-01-01T00:00 read as if it were UTC.
    pub start_local_ms: i64,
    /// Local time minus UTC, so the true epoch is `start_local_ms / 1000 - utc_offset_sec`.
    pub utc_offset_sec: i32,
    pub firmware_api: (u8, u8),
    pub net_mode: NetMode,
    pub random_seed: Option<u64>,
    pub locale: String,
    pub battery: Battery,
    /// The app folder as the host names it, for the missing-manifest message.
    pub app_dir_label: String,
    /// The app id when the manifest has none: the letters of the folder name.
    pub fallback_app_id: String,
    /// `state.json` as the host names it, for the unreadable-state warnings.
    pub state_label: String,
    /// Why the host could not read `state.json` (not a missing file), worded as 0.1's OSError.
    pub state_read_error: Option<String>,
    /// `<fixtures>/reader.json` for `ctx.reader`.
    pub reader_json: Option<Vec<u8>>,
    /// Recorded network fixtures by file name.
    pub fixtures: BTreeMap<String, Vec<u8>>,
    /// Lua instructions one outermost callback may run; kept across reboots of the Lua state.
    pub instruction_budget: i64,
    /// Total bytes and number of files the app may keep in `ctx.storage`.
    pub storage_quota: (u64, usize),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            start_local_ms: 0,
            utc_offset_sec: 0,
            firmware_api: (2, 26),
            net_mode: NetMode::Replay,
            random_seed: None,
            locale: "en".into(),
            battery: Battery::default(),
            app_dir_label: "the app folder".into(),
            fallback_app_id: "app".into(),
            state_label: "state.json".into(),
            state_read_error: None,
            reader_json: None,
            fixtures: BTreeMap::new(),
            instruction_budget: crate::lua::INSTRUCTION_BUDGET,
            storage_quota: STORAGE_QUOTA,
        }
    }
}

/// App data that outlives a run: `state.json`, `lock.json` and the app's files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Persisted {
    pub state_json: Option<Vec<u8>>,
    pub lock_json: Option<Vec<u8>>,
    pub files: BTreeMap<String, Vec<u8>>,
}
