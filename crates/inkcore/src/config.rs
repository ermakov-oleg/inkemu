use std::collections::BTreeMap;

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
    pub reader_json: Option<Vec<u8>>,
    /// Recorded network fixtures by file name.
    pub fixtures: BTreeMap<String, Vec<u8>>,
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
            reader_json: None,
            fixtures: BTreeMap::new(),
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
