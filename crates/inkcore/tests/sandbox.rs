use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};

fn boot(index_lua: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    Core::new(files, Config::default(), Persisted::default())
}

fn eval(core: &mut Core, code: &str) -> serde_json::Value {
    core.lua_eval(code).unwrap()
}

#[test]
fn package_loaded_has_only_the_device_libraries() {
    let mut core = boot("");
    let keys = eval(
        &mut core,
        "local o = {} for k in pairs(package.loaded) do o[#o + 1] = k end table.sort(o) return table.concat(o, ' ')",
    );
    assert_eq!(keys, "_G coroutine math package string table utf8");
    let err = core.lua_exec("require('python')").unwrap_err();
    assert!(err.contains("module 'python' not found"), "{err}");
}

#[test]
fn host_libraries_are_absent() {
    let mut core = boot("");
    for name in [
        "io",
        "os",
        "dofile",
        "loadfile",
        "collectgarbage",
        "debug",
        "print",
        "package.loadlib",
        "package.cpath",
        "package.searchpath",
        "package.loaded.io",
        "package.loaded.os",
        "package.loaded.debug",
        "python",
    ] {
        assert_eq!(
            eval(&mut core, &format!("return {name}")),
            serde_json::Value::Null,
            "{name}"
        );
    }
    for m in ["os", "io", "debug", "nonexistent"] {
        assert!(core.lua_exec(&format!("require('{m}')")).is_err(), "{m}");
    }
    assert_eq!(eval(&mut core, "return load('return 1 + 1')()"), 2);
    assert!(core.lua_exec("load(function() return nil end)").is_err());
    // load of a binary chunk: nil plus a message
    assert_eq!(
        eval(
            &mut core,
            "return select('#', load(string.dump(function() end)))"
        ),
        2
    );
}

#[test]
fn dot_call_on_ctx_method_names_the_colon_and_the_app_line() {
    let core = boot("function on_load(c) ctx.invalidate() end\n");
    let err = &core.errors()[0];
    assert!(
        err.contains("bad argument #2 to 'invalidate' (call ctx:invalidate with a colon)"),
        "{err}"
    );
    assert!(err.contains("index.lua:1:"), "{err}");
    assert!(!core.status().running);
}

#[test]
fn a_colon_call_works_and_another_table_is_not_the_object() {
    let mut core = boot("function on_load(c) c:log('hi', 1, nil, true) end\n");
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    assert!(core
        .log()
        .iter()
        .any(|l| l.ends_with("[app] hi 1 nil true")));
    let err = core.lua_exec("ctx.log({}, 'x')").unwrap_err();
    assert!(err.contains("colon"), "{err}");
    let err = core
        .lua_exec(
            "local fake = setmetatable({}, {__index = function() return 'ctx' end}) ctx.log(fake, 'x')",
        )
        .unwrap_err();
    assert!(err.contains("colon"), "{err}");
}
