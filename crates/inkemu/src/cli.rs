//! The `inkemu` command line: the parser and the client of the daemon.
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::command::{self, Reply, RunError};
use crate::daemon::{self, DEFAULT_PORT};
use crate::{paths, Session, SessionOptions};

const NO_DAEMON: &str =
    "no daemon running here; start one with `inkemu serve APP` or `inkemu window APP`";

fn common(c: Command) -> Command {
    c.arg(
        Arg::new("app")
            .required(true)
            .help("app directory with manifest.json"),
    )
    .arg(
        Arg::new("net")
            .long("net")
            .value_parser(["replay", "record", "live"])
            .default_value("replay"),
    )
    .arg(
        Arg::new("fresh")
            .long("fresh")
            .action(ArgAction::SetTrue)
            .help("start from an empty state"),
    )
    .arg(
        Arg::new("time")
            .long("time")
            .help("virtual start time, ISO 8601"),
    )
    .arg(
        Arg::new("state")
            .long("state")
            .help("state directory (default: .emu/<app_id>)"),
    )
    .arg(
        Arg::new("fixtures")
            .long("fixtures")
            .help("fixtures directory (default: tests/fixtures/<app_id>)"),
    )
}

fn daemon_port() -> Arg {
    Arg::new("port")
        .long("port")
        .value_parser(clap::value_parser!(u16))
        .default_value(DEFAULT_PORT.to_string())
        .help("daemon port")
}

fn parser() -> Command {
    let mut cmd = Command::new("inkemu")
        .about("Emulator of Xteink X4 Pro InkApp Lua apps")
        .subcommand_required(true)
        .arg_required_else_help(false)
        .disable_help_subcommand(true)
        .disable_version_flag(true);
    cmd = cmd
        .subcommand(
            common(Command::new("window").about("open the emulator window and the command daemon"))
                .arg(daemon_port()),
        )
        .subcommand(
            common(Command::new("serve").about("headless daemon in the foreground"))
                .arg(daemon_port()),
        )
        .subcommand(
            common(Command::new("run").about("one-shot run of a step script, no window"))
                .arg(
                    Arg::new("steps")
                        .long("steps")
                        .required(true)
                        .help("e.g. \"tap-text 'Tap me'; wait 61s; shot\""),
                )
                .arg(
                    Arg::new("out")
                        .long("out")
                        .default_value(".emu/run")
                        .help("directory for shots and texts.json"),
                ),
        );
    for (verb, usage) in command::verbs() {
        cmd = cmd.subcommand(
            Command::new(verb)
                .about(format!("send to the running daemon: {usage}"))
                .allow_negative_numbers(true)
                .arg(Arg::new("words").num_args(0..).help("arguments, see usage"))
                .arg(
                    Arg::new("port")
                        .long("port")
                        .value_parser(clap::value_parser!(u16))
                        .help("daemon port (default: from daemon.json)"),
                ),
        );
    }
    cmd
}

pub fn main(args: Vec<OsString>) -> i32 {
    let out = std::io::stdout();
    let err = std::io::stderr();
    run_cli(args, &paths::state_root(), &mut out.lock(), &mut err.lock())
}

pub fn run_cli(
    args: Vec<OsString>,
    state_root: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let mut argv = vec![OsString::from("inkemu")];
    argv.extend(args);
    let matches = match parser().try_get_matches_from(argv) {
        Ok(m) => m,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            let text = e.render().to_string();
            let _ = if code == 0 {
                write!(out, "{text}")
            } else {
                write!(err, "{text}")
            };
            return code;
        }
    };
    let Some((name, sub)) = matches.subcommand() else {
        return 2;
    };
    match name {
        "window" | "serve" | "run" => session_command(name, sub, state_root, out, err),
        verb => {
            let words = sub
                .get_many::<String>("words")
                .map(|w| w.cloned().collect())
                .unwrap_or_default();
            client(
                verb,
                words,
                sub.get_one::<u16>("port").copied(),
                state_root,
                out,
                err,
            )
        }
    }
}

fn fail(err: &mut dyn Write, msg: &str, code: i32) -> i32 {
    let _ = writeln!(err, "inkemu: {msg}");
    code
}

fn client(
    verb: &str,
    words: Vec<String>,
    port: Option<u16>,
    state_root: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let mut all = vec![verb.to_owned()];
    all.extend(words);
    let (cmd, mut args) = match command::parse_words(&all) {
        Ok(parsed) => parsed,
        Err(e) => return fail(err, &e, 2),
    };
    if cmd == "shot" {
        // the daemon has another working directory
        if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
            let abs = paths::absolute(Path::new(p));
            args.insert("path".into(), abs.to_string_lossy().into());
        }
    }
    let port = match port {
        Some(p) => p,
        None => match daemon::read_info(state_root) {
            Some(info) => info.port,
            None => return fail(err, NO_DAEMON, 2),
        },
    };
    let value = match daemon::post_command(port, &cmd, &args) {
        Ok(v) => v,
        Err(e) => return fail(err, &e, 2),
    };
    let reply = Reply::from_value(value);
    let _ = writeln!(out, "{}", command::format_result(&cmd, &reply));
    if reply.ok != Some(true) {
        let text = match &reply.error {
            Some(Some(e)) => e.clone(),
            _ => "the daemon reported an error".to_owned(),
        };
        return fail(err, &text, 1);
    }
    0
}

fn opt(sub: &ArgMatches, name: &str) -> Option<String> {
    sub.get_one::<String>(name).cloned()
}

fn session_command(
    name: &str,
    sub: &ArgMatches,
    state_root: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    if name != "run" {
        if let Some(live) = daemon::read_info(state_root) {
            return fail(
                err,
                &format!(
                    "a daemon is already running (pid {}, port {}, app {})",
                    live.pid, live.port, live.app
                ),
                2,
            );
        }
    }
    if name == "window" {
        return fail(
            err,
            "the window needs the Python package: run it as `python -m inkemu window ...`",
            2,
        );
    }
    let app = opt(sub, "app").unwrap_or_default();
    if !Path::new(&app).is_dir() {
        return fail(err, &format!("no such app directory: {app}"), 2);
    }
    let opts = SessionOptions {
        state_dir: opt(sub, "state").map(PathBuf::from),
        fixtures_dir: opt(sub, "fixtures").map(PathBuf::from),
        net: opt(sub, "net").unwrap_or_default(),
        fresh: sub.get_flag("fresh"),
        start_time: opt(sub, "time"),
        ..SessionOptions::default()
    };
    let mut s = match Session::open(Path::new(&app), opts) {
        Ok(s) => s,
        Err(e) => return fail(err, &e, 2),
    };
    if name == "serve" {
        let port = sub.get_one::<u16>("port").copied().unwrap_or(DEFAULT_PORT);
        if let Err(e) = daemon::serve(&mut s, port) {
            return fail(err, &e, 2);
        }
        return i32::from(!s.core().errors().is_empty());
    }
    let steps = opt(sub, "steps").unwrap_or_default();
    let out_dir = PathBuf::from(opt(sub, "out").unwrap_or_default());
    match command::run_steps(&mut s, &steps, &out_dir, out) {
        Ok(_) => {}
        Err(RunError::Usage(e)) => return fail(err, &e, 2),
        Err(RunError::Step(e)) => return fail(err, &format!("step failed: {e}"), 1),
    }
    for w in s.core().warnings().iter() {
        let _ = writeln!(err, "WARN: {w}");
    }
    let errors = s.core().errors();
    for e in errors.iter() {
        let _ = writeln!(err, "ERROR: {e}");
    }
    i32::from(!errors.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn cli(args: &[&str]) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let root = tempfile::tempdir().unwrap();
        let code = run_cli(
            args.iter().map(OsString::from).collect(),
            root.path(),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }

    fn hello() -> String {
        format!("{}/../../examples/hello", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn help_exits_zero_on_stdout() {
        for args in [&["--help"][..], &["run", "-h"], &["tap", "--help"]] {
            let (code, out, err) = cli(args);
            assert_eq!(code, 0, "{args:?}");
            assert!(out.contains("inkemu") || out.contains("Usage"), "{out}");
            assert!(err.is_empty());
        }
        let (_, out, _) = cli(&["run", "--help"]);
        assert!(out.contains("directory for shots and texts.json"), "{out}");
    }

    #[test]
    fn parse_errors_exit_two() {
        for args in [
            &[][..],
            &["bogus"],
            &["run", &hello(), "--net", "weird", "--steps", "shot"],
            &["run", &hello()],
            &["serve", &hello(), "--port", "abc"],
            &["tap", "--port"],
        ] {
            let (code, out, err) = cli(args);
            assert_eq!(code, 2, "{args:?}");
            assert!(out.is_empty() && !err.is_empty());
        }
    }

    #[test]
    fn non_utf8_argument_is_a_usage_error() {
        use std::os::unix::ffi::OsStringExt;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let bad = OsString::from_vec(vec![b'x', 0xff]);
        let root = tempfile::tempdir().unwrap();
        let code = run_cli(
            vec!["tap-text".into(), bad],
            root.path(),
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2);
    }

    #[test]
    fn negative_numbers_and_port_among_words() {
        let m = parser()
            .try_get_matches_from(["inkemu", "tap", "-5", "10", "--port", "7", "extra"])
            .unwrap();
        let (_, sub) = m.subcommand().unwrap();
        let words: Vec<_> = sub.get_many::<String>("words").unwrap().collect();
        assert_eq!(words, ["-5", "10", "extra"]);
        assert_eq!(sub.get_one::<u16>("port"), Some(&7));
        let m = parser()
            .try_get_matches_from(["inkemu", "tap", "--port=9", "1", "2"])
            .unwrap();
        assert_eq!(m.subcommand().unwrap().1.get_one::<u16>("port"), Some(&9));
    }

    #[test]
    fn long_options_take_equals_but_no_prefixes() {
        let m = parser()
            .try_get_matches_from([
                "inkemu",
                "serve",
                "app",
                "--fresh",
                "--net=live",
                "--port",
                "0",
            ])
            .unwrap();
        let (_, sub) = m.subcommand().unwrap();
        assert!(sub.get_flag("fresh"));
        assert_eq!(sub.get_one::<String>("net").unwrap(), "live");
        assert_eq!(sub.get_one::<u16>("port"), Some(&0));
        for args in [
            &["serve", "app", "--fre"][..],
            &["serve", "app", "--ne=live"],
            &["tap", "1", "2", "--por", "7"],
        ] {
            let (code, out, err) = cli(args);
            assert_eq!(code, 2, "{args:?}");
            assert!(
                out.is_empty() && err.contains("unexpected argument"),
                "{err}"
            );
        }
    }

    #[test]
    fn there_is_no_help_subcommand() {
        let (code, out, err) = cli(&["help"]);
        assert_eq!(code, 2);
        assert!(
            out.is_empty() && err.contains("unrecognized subcommand 'help'"),
            "{err}"
        );
    }

    #[test]
    fn a_missing_app_directory_is_a_usage_error() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-app");
        let file = tmp.path().join("file");
        fs::write(&file, b"").unwrap();
        for app in [&missing, &file] {
            let app = app.to_str().unwrap();
            for args in [
                vec!["run", app, "--steps", "shot", "--state", app],
                vec!["serve", app, "--port", "0", "--state", app],
            ] {
                let (code, out, err) = cli(&args);
                assert_eq!(code, 2, "{args:?}");
                assert!(out.is_empty());
                assert_eq!(err, format!("inkemu: no such app directory: {app}\n"));
            }
        }
        assert!(!missing.exists());
    }

    #[test]
    fn window_is_refused_with_code_two() {
        let (code, _, err) = cli(&["window", &hello()]);
        assert_eq!(code, 2);
        assert!(err.starts_with("inkemu: ") && err.contains("python -m inkemu window"));
    }

    fn run_args(tmp: &Path, steps: &str) -> Vec<String> {
        let p = |n: &str| tmp.join(n).to_string_lossy().into_owned();
        vec![
            "run".into(),
            hello(),
            "--fresh".into(),
            "--state".into(),
            p("state"),
            "--fixtures".into(),
            p("fx"),
            "--out".into(),
            p("out"),
            "--steps".into(),
            steps.into(),
        ]
    }

    fn run(tmp: &Path, steps: &str) -> (i32, String, String) {
        let args = run_args(tmp, steps);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        cli(&refs)
    }

    #[test]
    fn run_hello_writes_texts_and_shot() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, err) = run(tmp.path(), "tap-text 'Tap me'; texts; shot");
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("Taps: 1"), "{out}");
        assert!(tmp.path().join("out/001.png").exists());
        let texts = fs::read_to_string(tmp.path().join("out/texts.json")).unwrap();
        assert!(texts.contains("Taps: 1"));
    }

    #[test]
    fn run_usage_and_step_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, _, err) = run(tmp.path(), "jump 1");
        assert_eq!(code, 2);
        assert!(err.starts_with("inkemu: "));
        assert!(!tmp.path().join("out/texts.json").exists());
        let (code, _, err) = run(tmp.path(), "tap-text Nope; shot");
        assert_eq!(code, 1);
        assert!(err.starts_with("inkemu: step failed: "), "{err}");
        assert!(tmp.path().join("out/texts.json").exists());
    }

    #[test]
    fn a_failure_without_an_error_text_says_so() {
        use std::io::Read;
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port().to_string();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = c.read(&mut buf);
            let body = r#"{"ok": false}"#;
            let _ = write!(
                c,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = c.read(&mut buf);
        });
        let (code, _, err) = cli(&["state", "--port", &port]);
        h.join().unwrap();
        assert_eq!(code, 1);
        assert_eq!(err, "inkemu: the daemon reported an error\n");
    }

    #[test]
    fn unreachable_daemon_is_a_usage_error() {
        // 1 is never a daemon port, so the connection fails
        let (code, _, err) = cli(&["texts", "--port", "1"]);
        assert_eq!(code, 2, "{err}");
    }
}
