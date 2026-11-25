//! `inkemu._core`: the native `Session` for the Python `Emu` wrapper (`src/inkemu/emu.py`).
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

use inkemu::{Session as Native, SessionOptions};
use pyo3::exceptions::{PyLookupError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

struct SessionCell(Native);

// Lua has no thread affinity; the mutex serialises all access.
unsafe impl Send for SessionCell {}

type Status = (
    String,
    &'static str,
    usize,
    bool,
    (u32, u32),
    String,
    bool,
    Option<(&'static str, String, String)>,
    bool,
    Option<&'static str>,
    Vec<&'static str>,
);
type Text = (String, i32, i32, i32, i32, &'static str);

/// (width, height, PIL raw mode, bytes): mode "L" for a gray frame, else mode "1" of `frame()`.
fn pack(
    gray: Option<inkcore::Frame>,
    frame: impl FnOnce() -> inkcore::Frame,
) -> (u32, u32, &'static str, Vec<u8>) {
    match gray {
        Some(g) => (g.w, g.h, "L", g.gray_bytes()),
        None => {
            let f = frame();
            (f.w, f.h, "1", f.packed())
        }
    }
}

#[pyclass(module = "inkemu._core")]
struct Session {
    cell: Mutex<SessionCell>,
}

impl Session {
    /// Runs `f` on the session with the GIL released; the lock is taken and dropped inside, so a thread
    /// waiting for it never holds the GIL.
    fn with<T: Send>(&self, py: Python<'_>, f: impl FnOnce(&mut Native) -> T + Send) -> T {
        py.detach(|| {
            let mut guard = self.cell.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut guard.0)
        })
    }
}

#[pymethods]
impl Session {
    #[new]
    #[pyo3(signature = (app_dir, *, state_dir=None, fixtures_dir=None, net="replay".to_string(), start_time=None,
        firmware_api=(2, 33), fresh=false, secrets_file=None, seed_files=None, random_seed=None,
        instruction_budget=None, storage_quota=None, fonts_dir=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        app_dir: PathBuf,
        state_dir: Option<PathBuf>,
        fixtures_dir: Option<PathBuf>,
        net: String,
        start_time: Option<String>,
        firmware_api: (u8, u8),
        fresh: bool,
        secrets_file: Option<PathBuf>,
        seed_files: Option<bool>,
        random_seed: Option<String>,
        instruction_budget: Option<i64>,
        storage_quota: Option<(u64, usize)>,
        fonts_dir: Option<PathBuf>,
    ) -> PyResult<Self> {
        let opts = SessionOptions {
            state_dir,
            fixtures_dir,
            net,
            start_time,
            firmware_api,
            fresh,
            secrets_file,
            seed_files,
            random_seed,
            instruction_budget,
            storage_quota,
            fonts_dir,
        };
        let s = py
            .detach(|| Native::open(&app_dir, opts).map(SessionCell))
            .map_err(PyValueError::new_err)?;
        Ok(Session {
            cell: Mutex::new(s),
        })
    }

    fn pointer(&self, py: Python<'_>, phase: String, x: i32, y: i32) -> PyResult<()> {
        self.with(py, |s| s.pointer(&phase, x, y))
            .map_err(PyValueError::new_err)
    }

    fn tap(&self, py: Python<'_>, x: i32, y: i32) -> PyResult<()> {
        self.with(py, |s| s.tap(x, y))
            .map_err(PyValueError::new_err)
    }

    fn double_tap(&self, py: Python<'_>, x: i32, y: i32) -> PyResult<()> {
        self.with(py, |s| s.double_tap(x, y))
            .map_err(PyValueError::new_err)
    }

    fn hold(&self, py: Python<'_>, x: i32, y: i32, ms: i64) -> PyResult<()> {
        self.with(py, |s| s.hold(x, y, ms))
            .map_err(PyValueError::new_err)
    }

    fn drag(&self, py: Python<'_>, points: Vec<(i32, i32)>, duration_ms: i64) -> PyResult<()> {
        self.with(py, |s| s.drag(&points, duration_ms))
            .map_err(PyValueError::new_err)
    }

    fn swipe(
        &self,
        py: Python<'_>,
        direction: String,
        at: (i32, i32),
        distance: i32,
    ) -> PyResult<()> {
        self.with(py, |s| s.swipe(&direction, at, distance))
            .map_err(PyValueError::new_err)
    }

    fn tap_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        self.with(py, |s| s.tap_text(&text))
            .map_err(PyLookupError::new_err)
    }

    fn key(&self, py: Python<'_>, name: String) -> PyResult<()> {
        self.with(py, |s| s.key(&name))
            .map_err(PyValueError::new_err)
    }

    fn type_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        self.with(py, |s| s.type_text(&text))
            .map_err(PyRuntimeError::new_err)
    }

    fn submit_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        self.with(py, |s| s.submit_text(&text))
            .map_err(PyRuntimeError::new_err)
    }

    fn answer(&self, py: Python<'_>, ok: bool) -> PyResult<()> {
        self.with(py, |s| s.answer(ok))
            .map_err(PyRuntimeError::new_err)
    }

    fn wait(&self, py: Python<'_>, ms: i64) -> PyResult<()> {
        self.with(py, |s| s.wait(ms)).map_err(PyValueError::new_err)
    }

    fn advance(&self, py: Python<'_>, ms: i64) -> PyResult<()> {
        self.with(py, |s| s.advance(ms))
            .map_err(PyValueError::new_err)
    }

    fn leave(&self, py: Python<'_>) {
        self.with(py, |s| s.leave());
    }

    fn restart(&self, py: Python<'_>) {
        self.with(py, |s| s.restart());
    }

    fn reload(&self, py: Python<'_>) {
        self.with(py, |s| s.reload());
    }

    fn lock(&self, py: Python<'_>) {
        self.with(py, |s| s.lock())
    }

    fn unlock(&self, py: Python<'_>) {
        self.with(py, |s| s.unlock())
    }

    fn resume(&self, py: Python<'_>) {
        self.with(py, |s| s.resume());
    }

    fn set_paused(&self, py: Python<'_>, paused: bool) {
        self.with(py, |s| s.set_paused(paused));
    }

    fn set_locale(&self, py: Python<'_>, locale: String) {
        self.with(py, |s| s.set_locale(&locale));
    }

    fn log_line(&self, py: Python<'_>, msg: String) {
        self.with(py, |s| s.log_line(&msg));
    }

    fn lua_exec(&self, py: Python<'_>, code: String) -> PyResult<()> {
        self.with(py, |s| s.lua_exec(&code))
            .map_err(PyRuntimeError::new_err)
    }

    /// The first result as JSON text.
    fn lua_eval(&self, py: Python<'_>, code: String) -> PyResult<String> {
        self.with(py, |s| s.lua_eval(&code).map(|v| v.to_string()))
            .map_err(PyRuntimeError::new_err)
    }

    fn status(&self, py: Python<'_>) -> Status {
        self.with(py, |s| {
            let st = s.status();
            (
                st.time,
                st.net,
                st.frame_commands,
                st.paused,
                st.screen,
                st.app_id,
                st.running,
                st.pending.map(|p| (p.kind, p.title, p.text)),
                st.locked,
                st.refresh,
                st.refreshes,
            )
        })
    }

    fn texts(&self, py: Python<'_>) -> Vec<Text> {
        self.with(py, |s| {
            s.core()
                .texts()
                .into_iter()
                .map(|t| (t.text, t.x, t.y, t.w, t.h, t.font))
                .collect()
        })
    }

    /// The screen with plates, as (width, height, PIL mode, bytes): "L" for a gray frame, else "1".
    fn frame<'py>(
        &self,
        py: Python<'py>,
        dialog: bool,
    ) -> (u32, u32, &'static str, Bound<'py, PyBytes>) {
        let (w, h, mode, px) = self.with(py, |s| pack(s.core().frame_gray(), || s.frame(dialog)));
        (w, h, mode, PyBytes::new(py, &px))
    }

    /// The app's own frame without plates, always mode "1" (a gray frame's threshold).
    fn raw_frame<'py>(&self, py: Python<'py>) -> (u32, u32, &'static str, Bound<'py, PyBytes>) {
        let (w, h, mode, px) = self.with(py, |s| pack(None, || s.core().raw_frame()));
        (w, h, mode, PyBytes::new(py, &px))
    }

    fn log(&self, py: Python<'_>) -> Vec<String> {
        self.with(py, |s| s.core().log().to_vec())
    }

    fn errors(&self, py: Python<'_>) -> Vec<String> {
        self.with(py, |s| s.core().errors().to_vec())
    }

    fn warnings(&self, py: Python<'_>) -> Vec<String> {
        self.with(py, |s| s.core().warnings().to_vec())
    }

    fn toasts(&self, py: Python<'_>) -> Vec<String> {
        self.with(py, |s| s.core().toasts().to_vec())
    }

    fn clock_ms(&self, py: Python<'_>) -> i64 {
        self.with(py, |s| s.core().clock_ms())
    }

    /// (year, month, day, hour, minute, second, millisecond), local wall-clock time.
    fn now_local(&self, py: Python<'_>) -> (i32, u32, u32, u32, u32, u32, u32) {
        self.with(py, |s| s.core().now_local())
    }

    fn pending_id(&self, py: Python<'_>) -> u64 {
        self.with(py, |s| s.core().pending_id())
    }

    fn lock_wakes(&self, py: Python<'_>) -> u32 {
        self.with(py, |s| s.core().lock_wakes())
    }

    fn app_dir(&self, py: Python<'_>) -> PathBuf {
        self.with(py, |s| s.app_dir().to_path_buf())
    }

    fn state_dir(&self, py: Python<'_>) -> PathBuf {
        self.with(py, |s| s.state_dir().to_path_buf())
    }

    fn fixtures_dir(&self, py: Python<'_>) -> PathBuf {
        self.with(py, |s| s.fixtures_dir().to_path_buf())
    }

    fn secrets_file(&self, py: Python<'_>) -> PathBuf {
        self.with(py, |s| s.secrets_file().to_path_buf())
    }
}

/// 0.1 `clock.parse_duration`: "350", "350ms", "61s", "10m", "2h" in milliseconds.
#[pyfunction]
fn parse_duration(s: &str) -> PyResult<i64> {
    inkcore::clock::parse_duration(s).map_err(PyValueError::new_err)
}

/// The `inkemu` CLI; `args` excludes the program name. Returns the exit code.
#[pyfunction]
fn cli_main(py: Python<'_>, args: Vec<String>) -> i32 {
    py.detach(|| inkemu::cli::main(args.into_iter().map(OsString::from).collect()))
}

/// The stdio MCP server; `python` is the interpreter its daemon is spawned with.
#[pyfunction]
fn mcp_main(py: Python<'_>, python: PathBuf) -> i32 {
    py.detach(|| inkemu::mcp::main(Some(python)))
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", inkcore::VERSION)?;
    m.add("INSTRUCTION_BUDGET", inkcore::lua::INSTRUCTION_BUDGET)?;
    m.add("STORAGE_QUOTA", inkcore::STORAGE_QUOTA)?;
    m.add_class::<Session>()?;
    m.add_function(wrap_pyfunction!(parse_duration, m)?)?;
    m.add_function(wrap_pyfunction!(cli_main, m)?)?;
    m.add_function(wrap_pyfunction!(mcp_main, m)?)?;
    Ok(())
}
