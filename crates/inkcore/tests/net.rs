use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, NetMode, NetResult, Persisted};
use serde_json::json;
use sha2::{Digest, Sha256};

const DRAW: &str =
    "msg = 'none'\nfunction on_draw(c, g) g:clear(0); g:text(0, 0, 'msg ' .. tostring(msg)) end\n";

fn app(request_lua: &str, cb_body: &str) -> AppFiles {
    let manifest =
        r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "permissions": ["net"]}"#;
    let index = format!(
        "{DRAW}function on_load(c)\n  job = c.net:request({request_lua}, function(c, resp)\n{cb_body}\n    c:invalidate()\n  end)\nend\n"
    );
    AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index.into_bytes()),
    ]))
}

const STATUS_BODY: &str = "msg = resp.status .. ':' .. tostring(resp.body)";

fn fixture(req: serde_json::Value, resp: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"request": req, "match": "exact", "response": resp})).unwrap()
}

fn boot(files: AppFiles, cfg: Config) -> Core {
    Core::new(files, cfg, Persisted::default())
}

fn with_fixtures(fixtures: &[(&str, Vec<u8>)]) -> Config {
    Config {
        fixtures: fixtures
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        net_fixtures_label: "fx/net".into(),
        ..Config::default()
    }
}

fn msg(core: &mut Core) -> serde_json::Value {
    core.lua_eval("return msg").unwrap()
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn replay_hit_fires_after_300_ms_and_not_before() {
    let cfg = with_fixtures(&[(
        "hand.json",
        fixture(
            json!({"method": "GET", "url": "http://x.test/a"}),
            json!({"status": 200, "body": "ok"}),
        ),
    )]);
    let mut core = boot(app("{url = 'http://x.test/a'}", STATUS_BODY), cfg);
    assert_eq!(core.net_inflight(), 1);
    core.wait(299).unwrap();
    assert_eq!(msg(&mut core), "none");
    core.wait(1).unwrap();
    assert_eq!(msg(&mut core), "200:ok");
    assert_eq!(core.net_inflight(), 0);
    assert!(core.take_outgoing().is_empty());
    assert!(core
        .log()
        .iter()
        .any(|l| l.ends_with("net: GET http://x.test/a -> 200 (fixture)")));
}

#[test]
fn post_matches_by_body_hash() {
    let cfg = with_fixtures(&[(
        "p.json",
        fixture(
            json!({"method": "POST", "url": "http://x.test/p", "body_sha256": sha(b"q=1")}),
            json!({"status": 200, "body": "yes"}),
        ),
    )]);
    let req = |body: &str| format!("{{url = 'http://x.test/p', method = 'post', body = '{body}'}}");
    let mut core = boot(app(&req("q=1"), STATUS_BODY), cfg.clone());
    core.wait(300).unwrap();
    assert_eq!(msg(&mut core), "200:yes");
    let mut core = boot(app(&req("q=2"), STATUS_BODY), cfg);
    core.wait(300).unwrap();
    assert_eq!(msg(&mut core), "0:nil");
}

#[test]
fn no_fixture_names_method_url_and_record() {
    let mut core = boot(
        app(
            "{url = 'http://x.test/b'}",
            "msg = resp.err .. resp.error .. resp.status",
        ),
        with_fixtures(&[]),
    );
    core.wait(300).unwrap();
    assert_eq!(msg(&mut core), "no_fixtureno_fixture0");
    let want = "net: no fixture for GET http://x.test/b; run with --net record to save one, or write it by hand in fx/net";
    assert_eq!(core.warnings().to_vec(), vec![want.to_string()]);
}

#[test]
fn binary_fixture_body_reaches_lua_byte_exact() {
    let cfg = with_fixtures(&[
        (
            "b.json",
            fixture(
                json!({"method": "GET", "url": "http://x.test/bin"}),
                json!({"status": 200, "body_file": "sub/b.body"}),
            ),
        ),
        ("b.body", b"\xff\xfe\x00ok".to_vec()),
    ]);
    let mut core = boot(
        app(
            "{url = 'http://x.test/bin'}",
            "msg = {resp.bytes, resp.body:byte(1, -1)}",
        ),
        cfg,
    );
    core.wait(300).unwrap();
    assert_eq!(msg(&mut core), json!([5, 255, 254, 0, 111, 107]));
}

#[test]
fn live_requests_go_out_and_answers_come_back_in_virtual_time() {
    let cfg = Config {
        net_mode: NetMode::Live,
        ..Config::default()
    };
    let mut core = boot(
        app(
            "{url = 'http://x.test/up', method = 'POST', body = '\\255\\254', headers = {['X-N'] = 5}, timeout_ms = 1500}",
            "msg = {resp.status, resp.headers['x-a'], resp.body:byte(1, -1)}",
        ),
        cfg,
    );
    let out = core.take_outgoing();
    assert_eq!(out.len(), 1);
    let r = &out[0];
    assert_eq!(
        (r.method.as_str(), r.url.as_str(), r.timeout_ms),
        ("POST", "http://x.test/up", 1500)
    );
    assert_eq!(r.body, b"\xff\xfe");
    assert_eq!(r.headers, vec![("X-N".to_string(), "5".to_string())]);
    assert!(core.take_outgoing().is_empty());
    core.wait(100).unwrap();
    core.deliver(
        r.id,
        NetResult::Response {
            status: 201,
            headers: vec![("x-a".into(), "b".into())],
            body: b"\x00\xff\xfe".to_vec(),
            delay_ms: 300,
        },
    );
    core.wait(199).unwrap();
    assert_eq!(msg(&mut core), "none");
    core.wait(1).unwrap();
    assert_eq!(msg(&mut core), json!([201, "b", 0, 255, 254]));
    assert_eq!(core.net_inflight(), 0);
}

#[test]
fn a_late_answer_fires_at_once_and_a_cancelled_one_never() {
    let live = || Config {
        net_mode: NetMode::Live,
        ..Config::default()
    };
    let mut core = boot(app("{url = 'http://x.test/a'}", "msg = resp.err"), live());
    let id = core.take_outgoing()[0].id;
    core.wait(1000).unwrap();
    core.deliver(id, NetResult::Error("timeout".into()));
    assert_eq!(msg(&mut core), "timeout");
    assert_eq!(core.net_inflight(), 0);

    let mut core = boot(app("{url = 'http://x.test/a'}", "msg = resp.err"), live());
    let id = core.take_outgoing()[0].id;
    core.lua_exec("job:cancel()").unwrap();
    assert_eq!(core.net_inflight(), 0);
    core.deliver(id, NetResult::Error("timeout".into()));
    core.wait(1000).unwrap();
    assert_eq!(msg(&mut core), "none");
}

#[test]
fn replay_matches_the_redacted_url_and_never_logs_the_secret() {
    let cfg = Config {
        secrets: vec![("cal".into(), "private-abc123".into())],
        ..with_fixtures(&[(
            "c.json",
            fixture(
                json!({"method": "GET", "url": "http://x.test/ical/{{cal}}/basic.ics"}),
                json!({"status": 200, "body": "cal"}),
            ),
        )])
    };
    let mut core = boot(
        app(
            "{url = 'http://x.test/ical/private-abc123/basic.ics'}",
            STATUS_BODY,
        ),
        cfg,
    );
    core.wait(300).unwrap();
    assert_eq!(msg(&mut core), "200:cal");
    assert!(!core.log().iter().any(|l| l.contains("abc123")));
}
