//! The emulated firmware is API 2.33, build 26112001, as inventoried on the reader.
use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};
use serde_json::json;

fn start(index_lua: &str) -> Core {
    start_with(index_lua, Config::default().storage_quota)
}

fn start_with(index_lua: &str, storage_quota: (u64, usize)) -> Core {
    let manifest =
        r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    let cfg = Config {
        start_local_ms: inkcore::clock::local_ms(2026, 11, 20, 9, 0, 0, 0),
        utc_offset_sec: 3600,
        storage_quota,
        ..Config::default()
    };
    Core::new(files, cfg, Persisted::default())
}

fn boot(index_lua: &str) -> Core {
    let core = start(index_lua);
    assert!(core.errors().is_empty(), "{:?}", core.errors().to_vec());
    assert!(core.status().running);
    core
}

fn eval(core: &mut Core, code: &str) -> serde_json::Value {
    core.lua_eval(code)
        .unwrap_or_else(|e| panic!("{code}: {e}"))
}

#[test]
fn removed_members_are_nil() {
    let mut core = boot("");
    assert_eq!(
        eval(&mut core, "return {load == nil, ctx.display == nil}"),
        json!([true, true])
    );
}

#[test]
fn api_and_device_are_build_26112001() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local d = ctx.device return {ctx.api.major, ctx.api.minor, d.model, d.name, d.firmware, d.lock_timer}"
        ),
        json!([2, 33, "x4_pro", "book", "XT Licorice 26112001", true])
    );
}

#[test]
fn storage_usage_names_its_fields_as_the_device() {
    let mut core = boot("ctx.storage:write('a', '123')");
    assert_eq!(
        eval(
            &mut core,
            "local u = ctx.storage:usage() return {u.bytes, u.files, u.max_bytes, u.max_files, u.used_bytes == nil}"
        ),
        json!([3, 1, 134_217_728, 512, true])
    );
}

#[test]
fn clock_now_has_the_26112001_fields() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local t = ctx.clock:now()\n\
             return {t.wday == nil, t.yday == nil, t.weekday, t.date_sep, t.date_order, t.offset_minutes,\n\
               math.type(t.revision)}"
        ),
        // 2026-11-20 is a Friday: 5 counting from Sunday = 0
        json!([true, true, 5, "/", "dmy", 60, "integer"]),
        "API 2.33.1 contract §12 says otherwise (12/clock_weekday_sunday)"
    );
}

#[test]
fn clock_revision_changes_when_the_clock_is_moved() {
    let mut core = boot("");
    let before = eval(&mut core, "return ctx.clock:now().revision");
    assert_eq!(eval(&mut core, "return ctx.clock:now().revision"), before);
    core.wait(60_000).unwrap();
    let after = eval(&mut core, "return ctx.clock:now().revision");
    assert_ne!(after, before);
    core.tap(10, 10).unwrap();
    assert_eq!(eval(&mut core, "return ctx.clock:now().revision"), after);
    // real time passing (the window's ticks) is not a move of the clock
    core.advance(5_000).unwrap();
    assert_eq!(eval(&mut core, "return ctx.clock:now().revision"), after);
    assert_eq!(core.clock_ms(), 60_000 + 350 + 5_000);
}

#[test]
fn clock_revision_is_1_after_a_restart() {
    let mut core = boot("");
    core.wait(60_000).unwrap();
    assert_ne!(eval(&mut core, "return ctx.clock:now().revision"), json!(1));
    core.restart();
    assert_eq!(
        eval(&mut core, "return ctx.clock:now().revision"),
        json!(1),
        "§12: 1 at the start of the session"
    );
    core.advance(5_000).unwrap();
    assert_eq!(eval(&mut core, "return ctx.clock:now().revision"), json!(1));
}

#[test]
fn log_is_a_table_of_levels_and_still_callable() {
    let core = boot(
        "ctx.log:info('i1')\nctx.log:warn('w1')\nctx.log:error('e1')\nctx:log('y')\n\
         assert(type(ctx.log) == 'table')",
    );
    let log = core.log().join("\n");
    for want in [
        "[app] INFO i1",
        "[app] WARN w1",
        "[app] ERROR e1",
        "[app] y",
    ] {
        assert!(log.contains(want), "{want} not in {log}");
    }
    assert!(core.errors().is_empty() && core.warnings().is_empty());
}

#[test]
fn log_call_form_keeps_the_colon_check_at_the_app_line() {
    let core = start("function on_load(c)\n  ctx.log('x')\nend");
    let err = &core.errors()[0];
    assert!(
        err.contains("index.lua:2: bad argument #2 to 'log' (call ctx:log with a colon)"),
        "{err}"
    );
}

#[test]
fn hashes_are_raw_digests() {
    let mut core = boot("");
    let got = eval(
        &mut core,
        "local h = ctx.hash\n\
         local big = string.rep('x\\0y', 24000)\n\
         return {#h:sha256(''),\n\
           h:sha256('') == ctx.hex:decode('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'),\n\
           #h:md5('a\\0b'), h:md5('') == ctx.hex:decode('d41d8cd98f00b204e9800998ecf8427e'),\n\
           math.type(h:crc32('abc')), h:crc32('abc') == 891568578, h:crc32('a') == -390611389,\n\
           #h:hmac_sha256('k', 'm'),\n\
           h:hmac_sha256('key', 'The quick brown fox jumps over the lazy dog') ==\n\
             ctx.hex:decode('f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8'),\n\
           h:sha256(big) == ctx.hex:decode('a1f0f20c1dfc3b39665260b64051b1b50168dd415bd59f7e278b65207e9e65f6'),\n\
           h:md5(big) == ctx.hex:decode('e11602758c6ad817e5db08377a82ef11'),\n\
           h:crc32(big) == -634786208,\n\
           h:hmac_sha256('k', big) ==\n\
             ctx.hex:decode('c453329e9a04f350f7592095c299fa74279ddca0f711001bf6d05073f5ca4128'),\n\
           #h:sha1(''), h:sha1('') == ctx.hex:decode('da39a3ee5e6b4b0d3255bfef95601890afd80709'),\n\
           h:sha1('abc') == ctx.hex:decode('a9993e364706816aba3e25717850c26c9cd0d89d')}",
    );
    assert_eq!(
        got,
        json!([
            32, true, 16, true, "integer", true, true, 32, true, true, true, true, true, 20, true,
            true
        ])
    );
}

#[test]
fn files_and_storage_are_different_directories() {
    let mut core = boot(
        "ctx.files:write('a', '1')\nctx.storage:write('b', '2')\n\
         ctx.files:append('c', 'x')\nctx.files:append('c', 'yz')",
    );
    assert_eq!(
        eval(
            &mut core,
            "return {tostring(ctx.storage:read('a')), tostring(ctx.files:read('b')), ctx.files:read('a'),\n\
               ctx.storage:read('b'), ctx.files:read('c'), ctx.files.path, ctx.storage.append == nil,\n\
               ctx.files.usage == nil}"
        ),
        json!(["nil", "nil", "1", "2", "xyz", "/AppFiles/testapp", true, true])
    );
    let p = core.take_persist().unwrap();
    assert_eq!(
        p.files.keys().collect::<Vec<_>>(),
        ["b"],
        "storage holds only its own file"
    );
    assert_eq!(p.app_files.keys().collect::<Vec<_>>(), ["a", "c"]);
}

#[test]
fn only_storage_has_a_total_quota_and_both_cap_the_file_count() {
    let mut core = start_with("", (10, 2));
    assert_eq!(
        eval(
            &mut core,
            "local function r(ok, e) return tostring(ok) .. (e and ' ' .. e or '') end\n\
             local s = ctx.storage\n\
             local f = ctx.files\n\
             return {r(s:write('s1', '12345')), r(s:write('s2', '12345')), r(s:write('s3', '')),\n\
               r(s:write('s1', '123456')),\n\
               r(f:write('a', '1234')), r(f:append('a', '12345')), r(f:append('a', '12')), f:read('a'),\n\
               r(f:write('b', '1')), r(f:write('c', '')), r(f:append('new', ''))}"
        ),
        json!([
            "true",
            "true",
            "nil limit_exceeded",
            "nil limit_exceeded",
            "true",
            "true",
            "true",
            "12341234512",
            "true",
            "nil limit_exceeded",
            "nil limit_exceeded"
        ]),
        "API 2.33.1 §9 and §29 (09/storage_file_count_limit, 29/files_no_total_limit): only storage has a byte quota"
    );
}

#[test]
fn files_append_follows_the_write_rules() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local a, ae = ctx.files:append('bad:name', 'x')\n\
             local b, be = ctx.files:append('ok', {})\n\
             return {tostring(a), ae, tostring(b), be}"
        ),
        json!(["nil", "invalid_argument", "nil", "invalid_argument"]),
        "API 2.33.1 §29 (29/files_name_inner_space): a space is legal inside an app file name"
    );
}

#[test]
fn random_bytes_system_act_and_small_members() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local s = ctx.random:bytes(16)\n\
             local r, e = ctx.system:act('power_off')\n\
             ctx.frontlight:set(9)\nctx.frontlight:reset()\n\
             local caps = ctx.capabilities\n\
             return {type(s), #s, #ctx.random:bytes(1), tostring(r), e, ctx.frontlight:get(),\n\
               ctx.input.keyboard_h, ctx.input.pointer, ctx.screen.gray_levels, ctx.screen.gray_still,\n\
               next(ctx.package:cache_stats()) == nil, next(ctx.perf:info()) == nil,\n\
               caps.ble, caps.identity, caps.nn, caps.ui, caps.dialog}"
        ),
        json!([
            "string", 16, 1, "nil", "inactive", 0, 436, true, 2, 4, true, false, false, false, false,
            true, true
        ]),
        "API 2.33.1 contract §5 says otherwise: perf:info() is not empty (05/perf_info); random:bytes(0) is invalid_argument (26/random_bytes); §25: power_off outside a key or pointer up is inactive (25/act_inactive)"
    );
    assert!(
        core.log().iter().any(|l| l.contains("power_off")),
        "{:?}",
        core.log().to_vec()
    );
    let err = core.lua_exec("ctx.system:act()").unwrap_err();
    assert!(
        err.contains("bad argument #1 to 'act' (string expected)"),
        "{err}"
    );
}

#[test]
fn not_emulated_members_are_nil() {
    let mut core = boot("");
    for name in [
        "ctx.crypto",
        "ctx.nn",
        "ctx.widget",
        "ctx.dialog.choose",
        "ctx.i18n.t",
        "ctx.input.keys",
        "ctx.input.listen",
        "ctx.input.gamepad",
        "ctx.device.display",
        "ctx.device.keep_awake",
        "ctx.device.radios",
        "ctx.screen.occlusion",
    ] {
        assert_eq!(
            eval(&mut core, &format!("return {name} == nil")),
            json!(true),
            "{name}"
        );
    }
}

#[test]
fn manifests_up_to_2_33_load_and_above_is_refused() {
    let _ = boot("");
    let manifest = r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.34"}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), b"".to_vec()),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    assert!(
        core.errors()[0].contains("the emulated firmware has 2.33"),
        "{:?}",
        core.errors().to_vec()
    );
}

#[test]
fn random_bytes_takes_whole_numbers_only() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local function f(n) local r, e = ctx.random:bytes(n) return r and #r or e end\n\
             return {f(3.0), f(2.5), f(0/0), f(math.huge), f(-math.huge)}"
        ),
        json!([
            3,
            "invalid_argument",
            "invalid_argument",
            "invalid_argument",
            "invalid_argument"
        ]),
        "API 2.33.1 contract §0: only a whole float is an integer (26/random_bytes)"
    );
}

#[test]
fn url_query_takes_at_most_64_items() {
    let mut core = boot("");
    let query = |n: u32| {
        format!(
            "local t = {{}} for i = 1, {n} do t['k' .. i] = i end \
             local s, e = ctx.url:query(t) return {{s and #s or false, e or false}}"
        )
    };
    assert_eq!(eval(&mut core, &query(64))[1], json!(false));
    for n in [65, 100_000] {
        assert_eq!(
            eval(&mut core, &query(n)),
            json!([false, "limit_exceeded"]),
            "{n}"
        );
    }
}
