//! `Session`: an `inkcore::Core` plus the app folder and the state directory on disk.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use inkcore::app::read_manifest;
use inkcore::clock::local_ms;
use inkcore::log::py_repr_str;
use inkcore::{AppFiles, Battery, Config, Core, Fetcher, Frame, NetMode, Persisted, Status};

use crate::net_live::Net;
use crate::paths;
use crate::secrets;
use crate::state_disk;

const NET_MODES: [&str; 3] = ["replay", "record", "live"];

/// The arguments of 0.1 `Emu.__init__` after `app_dir`; the defaults are 0.1's.
pub struct SessionOptions {
    pub state_dir: Option<PathBuf>,
    pub fixtures_dir: Option<PathBuf>,
    pub net: String,
    /// ISO 8601; with an offset it is converted to local time. None: now.
    pub start_time: Option<String>,
    pub firmware_api: (u8, u8),
    pub fresh: bool,
    pub secrets_file: Option<PathBuf>,
    /// None: seed when `fresh`.
    pub seed_files: Option<bool>,
    pub random_seed: Option<String>,
    /// Lua instructions one callback may run; None: the default.
    pub instruction_budget: Option<i64>,
    /// Quota (bytes, files) of `ctx.storage` and, separately, of `ctx.files`; None: the default.
    pub storage_quota: Option<(u64, usize)>,
    /// A folder with `small.bdf`, `medium.bdf` and `large.bdf` for `ctx.fonts`; None: `$INKEMU_FONTS`, else the
    /// bundled DejaVu Sans.
    pub fonts_dir: Option<PathBuf>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions {
            state_dir: None,
            fixtures_dir: None,
            net: "replay".into(),
            start_time: None,
            firmware_api: (2, 33),
            fresh: false,
            secrets_file: None,
            seed_files: None,
            random_seed: None,
            instruction_budget: None,
            storage_quota: None,
            fonts_dir: None,
        }
    }
}

pub struct Session {
    core: Core,
    app_dir: PathBuf,
    state_dir: PathBuf,
    fixtures_dir: PathBuf,
    secrets_file: PathBuf,
    /// What is known to be on disk, so only changes are written.
    on_disk: Persisted,
    net_mode: NetMode,
}

/// Runs a mutating `Core` method, then writes the app data it changed. While locked, the secrets are read first:
/// a lock screen wake inside the call boots a new Lua state.
macro_rules! mutating {
    ($($(#[$doc:meta])* fn $name:ident(&mut self $(, $arg:ident: $ty:ty)*) $(-> $ret:ty)?;)*) => {
        $($(#[$doc])* pub fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? {
            if self.core.locked() {
                self.read_secrets();
            }
            let r = self.core.$name($($arg),*);
            self.flush();
            r
        })*
    };
}

impl Session {
    pub fn open(app_dir: &Path, opts: SessionOptions) -> Result<Session, String> {
        if !NET_MODES.contains(&opts.net.as_str()) {
            return Err(format!(
                "net must be one of {}, not {}",
                NET_MODES.join(", "),
                py_repr_str(&opts.net)
            ));
        }
        // parsed before anything on disk is touched, as in 0.1
        let (start_local_ms, utc_offset_sec) = start_time(opts.start_time.as_deref())?;
        let fonts = read_fonts(opts.fonts_dir.clone())?;
        let net_mode = match opts.net.as_str() {
            "record" => NetMode::Record,
            "live" => NetMode::Live,
            _ => NetMode::Replay,
        };
        let app_dir = paths::resolve(app_dir);
        let files = read_app_files(&app_dir);
        let (manifest, problem) =
            read_manifest(&files, opts.firmware_api, &app_dir.display().to_string());
        let app_id = paths::app_id_of(&app_dir, Some(&manifest));
        let root = paths::state_root();
        let state_dir = match &opts.state_dir {
            Some(d) => d.clone(),
            None => {
                let d = root.join(&app_id);
                paths::check_inside(&d, &root)?;
                d
            }
        };
        let fixtures_dir = opts
            .fixtures_dir
            .clone()
            .unwrap_or_else(|| paths::absolute(Path::new("tests/fixtures")).join(&app_id));
        let secrets_file = opts
            .secrets_file
            .clone()
            .unwrap_or_else(|| root.join("secrets.json"));
        if opts.fresh && problem.is_none() {
            state_disk::wipe(&state_dir);
        }
        if opts.seed_files.unwrap_or(opts.fresh) && problem.is_none() {
            state_disk::seed_files(&fixtures_dir, &state_dir);
        }
        let (on_disk, state_read_error) = state_disk::load(&state_dir);
        let secrets = secrets::load(&secrets_file);
        let net_dir = fixtures_dir.join("net");
        let cfg = Config {
            start_local_ms,
            utc_offset_sec,
            firmware_api: opts.firmware_api,
            net_mode,
            random_seed: opts.random_seed.as_deref().map(seed_of),
            locale: "en".into(),
            battery: Battery::default(),
            app_dir_label: app_dir.display().to_string(),
            fallback_app_id: paths::fallback_app_id(&app_dir),
            state_label: state_dir.join("state.json").display().to_string(),
            state_read_error,
            reader_json: fs::read(fixtures_dir.join("reader.json")).ok(),
            fixtures: read_fixtures(&net_dir),
            net_fixtures_label: net_dir.display().to_string(),
            secrets: secrets.values.clone(),
            secret_problems: secrets.problems,
            instruction_budget: opts
                .instruction_budget
                .unwrap_or(inkcore::lua::INSTRUCTION_BUDGET),
            storage_quota: opts.storage_quota.unwrap_or(inkcore::STORAGE_QUOTA),
            fonts,
        };
        let fetcher = fetcher(net_mode, secrets.values, net_dir);
        let core = Core::with_fetcher(files, cfg, on_disk.clone(), fetcher);
        let mut session = Session {
            core,
            app_dir,
            state_dir,
            fixtures_dir,
            secrets_file,
            on_disk,
            net_mode,
        };
        session.flush();
        Ok(session)
    }

    pub fn core(&self) -> &Core {
        &self.core
    }

    pub fn app_dir(&self) -> &Path {
        &self.app_dir
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn fixtures_dir(&self) -> &Path {
        &self.fixtures_dir
    }

    pub fn secrets_file(&self) -> &Path {
        &self.secrets_file
    }

    /// Writes the app data the core has changed since the last call.
    pub fn flush(&mut self) {
        let Some(new) = self.core.take_persist() else {
            return;
        };
        match state_disk::save(&self.state_dir, &new, &self.on_disk) {
            Ok(()) => self.on_disk = new,
            Err(e) => {
                let msg = format!(
                    "WARN cannot save app data to {}: {e}",
                    self.state_dir.display()
                );
                self.core.log_line(&msg);
            }
        }
    }

    /// Leaves and starts the app again with the secrets file read anew.
    pub fn restart(&mut self) {
        self.read_secrets();
        self.core.restart();
        self.flush();
    }

    /// Leaves the app and runs its lock screen, with the secrets file read anew.
    pub fn lock(&mut self) {
        self.read_secrets();
        self.core.lock();
        self.flush();
    }

    /// Starts the app again after the lock screen, with the secrets file read anew.
    pub fn unlock(&mut self) {
        self.read_secrets();
        self.core.unlock();
        self.flush();
    }

    /// Re-reads the secrets file for the core and the shell together, as 0.1 did at every start: a secret added
    /// after a warning is redacted from the next recording on, and a broken file refuses `record` again.
    fn read_secrets(&mut self) {
        let secrets = secrets::load(&self.secrets_file);
        let net_dir = self.fixtures_dir.join("net");
        let fetcher = fetcher(self.net_mode, secrets.values.clone(), net_dir);
        self.core.set_fetcher(fetcher);
        self.core.set_secrets(secrets.values, secrets.problems);
    }

    /// Re-reads the app folder, the fixtures and the secrets, then reloads: edited code takes effect.
    pub fn reload(&mut self) {
        self.core.set_app(read_app_files(&self.app_dir));
        self.core
            .set_fixtures(read_fixtures(&self.fixtures_dir.join("net")));
        self.read_secrets();
        self.core.reload();
        self.flush();
    }

    mutating! {
        fn pointer(&mut self, phase: &str, x: i32, y: i32) -> Result<(), String>;
        fn tap(&mut self, x: i32, y: i32) -> Result<(), String>;
        fn double_tap(&mut self, x: i32, y: i32) -> Result<(), String>;
        fn hold(&mut self, x: i32, y: i32, ms: i64) -> Result<(), String>;
        fn drag(&mut self, points: &[(i32, i32)], duration_ms: i64) -> Result<(), String>;
        fn swipe(&mut self, dir: &str, at: (i32, i32), distance: i32) -> Result<(), String>;
        fn tap_text(&mut self, text: &str) -> Result<(), String>;
        fn key(&mut self, name: &str) -> Result<(), String>;
        fn type_text(&mut self, text: &str) -> Result<(), String>;
        fn answer(&mut self, ok: bool) -> Result<(), String>;
        fn wait(&mut self, ms: i64) -> Result<(), String>;
        fn advance(&mut self, ms: i64) -> Result<(), String>;
        fn leave(&mut self);
        fn resume(&mut self);
        fn set_paused(&mut self, paused: bool);
        fn set_locale(&mut self, locale: &str);
        fn log_line(&mut self, msg: &str);
        fn lua_exec(&mut self, code: &str) -> Result<(), String>;
        fn lua_eval(&mut self, code: &str) -> Result<serde_json::Value, String>;
    }

    pub fn status(&self) -> Status {
        self.core.status()
    }

    pub fn frame(&self, dialog: bool) -> Frame {
        self.core.frame(dialog)
    }
}

/// The core's fetcher for `record`/`live`: real HTTP requests at request time, as 0.1; none in `replay`.
fn fetcher(mode: NetMode, secrets: Vec<(String, String)>, net_dir: PathBuf) -> Option<Fetcher> {
    if mode == NetMode::Replay {
        return None;
    }
    let net = Net::new(mode, secrets, net_dir);
    Some(Box::new(move |r| net.perform(r)))
}

/// The three BDF files of `dir`, else of `$INKEMU_FONTS` (empty is unset); a set folder must hold readable fonts.
fn read_fonts(dir: Option<PathBuf>) -> Result<Option<[Vec<u8>; 3]>, String> {
    let dir = dir.or_else(|| {
        std::env::var_os("INKEMU_FONTS")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    });
    let Some(dir) = dir else {
        return Ok(None);
    };
    let read = |name: &str| {
        let path = dir.join(format!("{name}.bdf"));
        fs::read(&path).map_err(|e| format!("fonts: cannot read {}: {e}", path.display()))
    };
    let files = [read("small")?, read("medium")?, read("large")?];
    inkcore::draw::text::Fonts::from_bdf(&files)
        .map_err(|e| format!("fonts: {}/{e}", dir.display()))?;
    Ok(Some(files))
}

/// Every file under the app folder, by POSIX path; symlinks that leave the folder are skipped (0.1 `_find_file`).
fn read_app_files(app_dir: &Path) -> AppFiles {
    fn walk(root: &Path, dir: &Path, out: &mut AppFiles) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(ty) = e.file_type() else { continue };
            if ty.is_dir() {
                if e.file_name() != ".git" {
                    walk(root, &path, out);
                }
                continue;
            }
            let inside = ty.is_file()
                || path
                    .canonicalize()
                    .is_ok_and(|real| real.starts_with(root) && real.is_file());
            let (Some(rel), true) = (path.strip_prefix(root).ok(), inside) else {
                continue;
            };
            let rel: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
            if let Ok(data) = fs::read(&path) {
                out.0.insert(rel.join("/"), data);
            }
        }
    }
    let mut out = AppFiles::default();
    walk(app_dir, app_dir, &mut out);
    out
}

/// The regular files of `<fixtures>/net/` by name.
fn read_fixtures(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let Ok(rd) = fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    rd.flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let data = fs::read(e.path()).ok()?;
            Some((e.file_name().to_string_lossy().into_owned(), data))
        })
        .collect()
}

/// FNV-1a: any text gives the same seed on every run.
fn seed_of(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// 0.1 `_parse_start`: (local wall-clock ms, local offset from UTC in seconds).
fn start_time(text: Option<&str>) -> Result<(i64, i32), String> {
    let Some(text) = text else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let off = local_offset_at_utc(now);
        return Ok(((now + i64::from(off)) * 1000, off));
    };
    let bad = || format!("Invalid isoformat string: {}", py_repr_str(text));
    let (date, rest) = match text.find(['T', ' ']) {
        Some(i) => (&text[..i], Some(&text[i + 1..])),
        None => (text, None),
    };
    let mut d = date.split('-');
    let num = |s: Option<&str>, len: usize| -> Option<u32> {
        s.filter(|s| s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))?
            .parse()
            .ok()
    };
    let (y, mo, day) = (num(d.next(), 4), num(d.next(), 2), num(d.next(), 2));
    let (Some(y), Some(mo), Some(day), None) = (y, mo, day, d.next()) else {
        return Err(bad());
    };
    if !(1..=12).contains(&mo) || day == 0 || day > days_in_month(y as i32, mo) {
        return Err(bad());
    }
    let (mut h, mut mi, mut s, mut ms) = (0, 0, 0, 0);
    let mut tz: Option<i32> = None;
    if let Some(rest) = rest {
        let (clock, zone) = match rest.find(['Z', '+', '-']) {
            Some(i) => (&rest[..i], Some(&rest[i..])),
            None => (rest, None),
        };
        let mut parts = clock.splitn(3, ':');
        h = num(parts.next(), 2).ok_or_else(bad)?;
        mi = num(parts.next(), 2).ok_or_else(bad)?;
        if let Some(sec) = parts.next() {
            let (whole, frac) = sec.split_once(['.', ',']).unwrap_or((sec, ""));
            s = num(Some(whole), 2).ok_or_else(bad)?;
            if !frac.bytes().all(|b| b.is_ascii_digit()) || frac.len() > 6 {
                return Err(bad());
            }
            ms = format!("{frac:0<3}")[..3].parse().map_err(|_| bad())?;
        }
        if h > 23 || mi > 59 || s > 59 {
            return Err(bad());
        }
        tz = match zone {
            None => None,
            Some("Z") => Some(0),
            Some(z) => {
                let sign = if z.starts_with('-') { -1 } else { 1 };
                let digits = z[1..].replace(':', "");
                if digits.len() < 2 || digits.len() % 2 != 0 || digits.len() > 6 {
                    return Err(bad());
                }
                let field = |i: usize| -> Result<i32, String> {
                    digits
                        .get(i..i + 2)
                        .map_or(Ok(0), |p| p.parse().map_err(|_| bad()))
                };
                Some(sign * (field(0)? * 3600 + field(2)? * 60 + field(4)?))
            }
        };
    }
    let wall = local_ms(y as i32, mo, day, h, mi, s, ms);
    Ok(match tz {
        // an aware time is converted to the machine's local time
        Some(off) => {
            let utc = wall.div_euclid(1000) - i64::from(off);
            let local = local_offset_at_utc(utc);
            (wall + (i64::from(local) - i64::from(off)) * 1000, local)
        }
        None => (wall, local_offset_of_wall(wall.div_euclid(1000))),
    })
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(unix)]
fn local_offset_at_utc(utc_sec: i64) -> i32 {
    let t = utc_sec as libc::time_t;
    // SAFETY: localtime_r writes only into the tm we pass.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
}

#[cfg(unix)]
fn local_offset_of_wall(wall_sec: i64) -> i32 {
    // reading the wall time as UTC gives the right fields for mktime's local interpretation
    let t = wall_sec as libc::time_t;
    // SAFETY: gmtime_r/mktime/localtime_r only touch the tm values we pass.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::gmtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_isdst = -1;
        let real = libc::mktime(&mut tm);
        if real == -1 {
            return 0;
        }
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&real, &mut local);
        local.tm_gmtoff as i32
    }
}

#[cfg(not(unix))]
fn local_offset_at_utc(_utc_sec: i64) -> i32 {
    0
}

#[cfg(not(unix))]
fn local_offset_of_wall(_wall_sec: i64) -> i32 {
    0
}
