//! The app's Lua state: sandbox, `require` from the app folder, instruction budget and protected calls.
use std::cell::Cell;
use std::rc::Rc;

use mlua::chunk::ChunkMode;
use mlua::debug::Debug;
use mlua::{
    Error, Function, HookTriggers, IntoLuaMulti, Lua, LuaOptions, LuaString, MultiValue, StdLib,
    VmState,
};

use crate::app::{find_module, AppFiles};

/// ~0.7 s of pure Lua on a laptop, far beyond what the reader's CPU runs in one callback; counted, so deterministic.
pub const INSTRUCTION_BUDGET: i64 = 200_000_000;
const HOOK_STEP: u32 = 10_000;

/// Chunk name of the host's own Lua code: exempt from the budget, hidden from tracebacks.
const HOST_SRC: &str = "=[host]";
const HOST_FRAME: &str = "[host]:";

const SANDBOX: &str = r#"
local raw_load, error, type = load, error, type
local HOST_SRC = ...
dofile, loadfile, collectgarbage, print = nil, nil, nil, nil
package.loadlib, package.cpath, package.path, package.searchpath = nil, nil, '', nil
for k in pairs(package.preload) do package.preload[k] = nil end
function load(chunk, name, mode, ...)
  if type(chunk) ~= 'string' then error("bad argument #1 to 'load' (only string chunks are allowed)", 2) end
  if name == HOST_SRC then name = '=(load)' end  -- host chunks are exempt from the budget check
  return raw_load(chunk, name, 't', ...)
end
local find
package.searchers = { function(name)
  if not find then return nil end
  local path, src = find(name)
  if not path then return src end
  local f, err = raw_load(src, '@' .. path, 't')
  if not f then error("error loading module '" .. name .. "' from file '" .. path .. "':\n\t" .. err, 0) end
  return f, path
end }
return function(f) find = f end
"#;

// Object methods: `objs` is weak and private, so an app cannot fake an object. A method body returns (true, ...) or
// (false, message): the error is raised here, at the app's line.
const OBJECTS: &str = r#"
local error, pack, unpack = error, table.pack, table.unpack
local objs = setmetatable({}, { __mode = 'k' })
return function(t, name) objs[t] = name end,
  function(obj_name, fn_name, fn)
    local where = obj_name == 'ctx' and 'ctx' or 'ctx.' .. obj_name
    local msg = "bad argument #2 to '" .. fn_name .. "' (call " .. where .. ":" .. fn_name .. " with a colon)"
    return function(self, ...)
      if objs[self] ~= obj_name then error(msg, 2) end
      local r = pack(fn(...))
      if r[1] then return unpack(r, 2, r.n) end
      error(r[2], 2)
    end
  end
"#;

struct Budget {
    units: Cell<i64>,
    left: Cell<i64>,
    exhausted: Cell<bool>,
    depth: Cell<u32>,
}

impl Budget {
    fn message(&self) -> String {
        format!(
            "callback exceeded the instruction budget ({} instructions) - infinite loop?",
            self.units.get() * i64::from(HOOK_STEP)
        )
    }
}

/// The only way to create a Lua state: device number types, device libraries, no way out of the sandbox.
pub fn new_lua() -> Lua {
    let libs = StdLib::TABLE
        | StdLib::STRING
        | StdLib::MATH
        | StdLib::UTF8
        | StdLib::COROUTINE
        | StdLib::PACKAGE;
    let lua = Lua::new_with(libs, LuaOptions::default()).expect("cannot create a Lua state");
    let ok: bool = lua
        .load("return math.maxinteger == 2147483647 and string.format('%.17g', 0.1) == '0.10000000149011612'")
        .set_name(HOST_SRC)
        .eval()
        .expect("number type check failed to run");
    assert!(
        ok,
        "Lua was built with the wrong number types: run scripts/vendor-lua.sh"
    );
    let set_find: Function = lua
        .load(SANDBOX)
        .set_name(HOST_SRC)
        .call(HOST_SRC)
        .expect("sandbox setup failed");
    lua.set_named_registry_value("inkemu.set_find", set_find)
        .expect("sandbox setup failed");
    let (register, method): (Function, Function) = lua
        .load(OBJECTS)
        .set_name(HOST_SRC)
        .call(())
        .expect("sandbox setup failed");
    lua.set_named_registry_value("inkemu.register_object", register)
        .expect("sandbox setup failed");
    lua.set_named_registry_value("inkemu.object_method", method)
        .expect("sandbox setup failed");
    lua.set_app_data(Rc::new(Budget {
        units: Cell::new(0),
        left: Cell::new(0),
        exhausted: Cell::new(false),
        depth: Cell::new(0),
    }));
    set_budget(&lua, INSTRUCTION_BUDGET);
    lua
}

/// Instructions one outermost callback may run before it is aborted.
pub fn set_budget(lua: &Lua, instructions: i64) {
    budget(lua)
        .units
        .set((instructions / i64::from(HOOK_STEP)).max(1));
}

fn budget(lua: &Lua) -> Rc<Budget> {
    lua.app_data_ref::<Rc<Budget>>()
        .expect("not a Lua state from new_lua")
        .clone()
}

/// Makes `require` load modules from the app folder.
pub fn install_require(lua: &Lua, files: Rc<AppFiles>) -> mlua::Result<()> {
    let find = lua.create_function(move |lua, name: LuaString| {
        let name = name.to_string_lossy();
        match find_module(&files, &name) {
            Ok((path, src)) => (path, lua.create_string(src)?).into_lua_multi(lua),
            Err(msg) => (mlua::Value::Nil, msg).into_lua_multi(lua),
        }
    })?;
    let set_find: Function = lua.named_registry_value("inkemu.set_find")?;
    set_find.call(find)
}

/// Compiles text Lua code as Lua's `load` would; the error is Lua's message.
pub fn load_chunk(lua: &Lua, src: &[u8], chunkname: &str) -> Result<Function, String> {
    lua.load(src)
        .set_name(chunkname)
        .set_mode(ChunkMode::Text)
        .into_function()
        .map_err(|e| error_text(&e))
}

fn hook(b: &Rc<Budget>, lua: &Lua, dbg: &Debug) -> mlua::Result<VmState> {
    if b.depth.get() == 0 {
        return Ok(VmState::Continue);
    }
    b.left.set(b.left.get() - 1);
    if b.left.get() >= 0 {
        return Ok(VmState::Continue);
    }
    let src = dbg.source();
    if src.source.as_deref() == Some(HOST_SRC) {
        return Ok(VmState::Continue);
    }
    b.exhausted.set(true);
    // fire on every instruction of the thread it fired on, so an app pcall cannot swallow the abort
    let b2 = b.clone();
    lua.set_global_hook(HookTriggers::new().every_nth_instruction(1), move |l, d| {
        hook(&b2, l, d)
    })?;
    let line = dbg.current_line().unwrap_or(0);
    let at = match src.short_src.as_deref() {
        Some(short) if src.source.as_deref().is_some_and(|s| s.starts_with('@')) && line > 0 => {
            format!("{short}:{line}: ")
        }
        _ => String::new(),
    };
    Err(Error::runtime(format!("{at}{}", b.message())))
}

/// Calls `f` like `xpcall`: the results, or the error message with a stack traceback. The outermost call gets a
/// fresh instruction budget; nested calls (a host function calling back into Lua) share it.
pub fn call_guarded(
    lua: &Lua,
    f: &Function,
    args: impl IntoLuaMulti,
) -> Result<MultiValue, String> {
    let b = budget(lua);
    if b.depth.get() > 0 {
        b.depth.set(b.depth.get() + 1);
        let _unwind = Unwind {
            lua,
            b: &b,
            outer: false,
        };
        return f.call::<MultiValue>(args).map_err(|e| error_text(&e));
    }
    b.left.set(b.units.get());
    b.exhausted.set(false);
    b.depth.set(1);
    // restores the depth and the hook even when a Rust panic unwinds through the Lua call
    let _unwind = Unwind {
        lua,
        b: &b,
        outer: true,
    };
    let b2 = b.clone();
    let hooked = lua.set_global_hook(
        HookTriggers::new().every_nth_instruction(HOOK_STEP),
        move |l, d| hook(&b2, l, d),
    );
    match hooked.and_then(|()| f.call::<MultiValue>(args)) {
        Ok(_) if b.exhausted.get() => Err(b.message()),
        Ok(v) => Ok(v),
        Err(e) => Err(error_text(&e)),
    }
}

struct Unwind<'a> {
    lua: &'a Lua,
    b: &'a Budget,
    outer: bool,
}

impl Drop for Unwind<'_> {
    fn drop(&mut self) {
        if self.outer {
            self.lua.remove_global_hook();
            self.b.depth.set(0);
        } else {
            self.b.depth.set(self.b.depth.get() - 1);
        }
    }
}

/// The text of a Lua error as an app developer reads it: message and traceback, host frames left out.
pub fn error_text(e: &Error) -> String {
    match e {
        // the traceback of mlua's message handler starts with the handler's own frame
        Error::RuntimeError(m) | Error::MemoryError(m) => clean_traceback(m, true),
        Error::SyntaxError { message, .. } => message.clone(),
        Error::CallbackError { traceback, cause } => {
            clean_traceback(&format!("{}\n{traceback}", error_text(cause)), false)
        }
        other => other.to_string(),
    }
}

fn clean_traceback(raw: &str, drop_first: bool) -> String {
    let Some((head, tb)) = raw.split_once("\nstack traceback:") else {
        return raw.to_string();
    };
    let frames = tb.split('\n').skip(1 + usize::from(drop_first));
    let kept: String = frames
        .filter(|l| !l.trim_start().starts_with(HOST_FRAME))
        .map(|l| format!("\n{l}"))
        .collect();
    format!("{head}\nstack traceback:{kept}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOP: &str = "while true do end";

    fn run(code: &str) -> Result<MultiValue, String> {
        let lua = new_lua();
        set_budget(&lua, 1_000_000);
        let f = load_chunk(
            &lua,
            format!("function on_load(c) {code} end").as_bytes(),
            "@index.lua",
        )
        .unwrap();
        call_guarded(&lua, &f, ()).unwrap();
        let on_load: Function = lua.globals().get("on_load").unwrap();
        call_guarded(&lua, &on_load, ())
    }

    #[test]
    fn app_cannot_swallow_the_abort() {
        for code in [
            format!("while true do pcall(function() {LOOP} end) end"),
            format!("return pcall(function() {LOOP} end)"),
            format!("while true do xpcall(function() {LOOP} end, function(e) return e end) end"),
            format!("coroutine.wrap(function() {LOOP} end)()"),
            format!("local co = coroutine.create(function() {LOOP} end) while true do coroutine.resume(co) end"),
            format!("pcall(coroutine.wrap(function() {LOOP} end)) {LOOP}"),
            format!("coroutine.wrap(function() while true do pcall(function() {LOOP} end) end end)()"),
            format!(
                "local function mk() return coroutine.wrap(function() while true do pcall(function() {LOOP} end) end end) end \
                 local a, b = mk(), mk() pcall(a) b()"
            ),
            format!(
                "setmetatable({{}}, {{__index = function() {LOOP} end}}).x = nil; \
                 local _ = setmetatable({{}}, {{__index = function() {LOOP} end}}).x"
            ),
        ] {
            let err = run(&code).unwrap_err();
            assert!(
                err.contains("callback exceeded the instruction budget (1000000 instructions) - infinite loop?"),
                "{code}: {err}"
            );
            assert!(!err.contains("error in error handling"), "{code}: {err}");
        }
    }

    #[test]
    fn a_rust_panic_in_a_host_function_leaves_the_budget_usable() {
        let lua = new_lua();
        set_budget(&lua, 1_000_000);
        let boom = lua
            .create_function(|_, ()| -> mlua::Result<()> { panic!("host bug") })
            .unwrap();
        lua.globals().set("boom", boom).unwrap();
        // burns most of the budget before the panic: a leaked depth would carry it into the next call
        let f = load_chunk(&lua, b"for _ = 1, 700000 do end boom()", "=burn").unwrap();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = call_guarded(&lua, &f, ());
        }));
        assert!(unwound.is_err());
        let fine = load_chunk(&lua, b"for _ = 1, 700000 do end return 'ok'", "=fine").unwrap();
        assert!(call_guarded(&lua, &fine, ()).is_ok());
        let runaway = load_chunk(&lua, LOOP.as_bytes(), "=runaway").unwrap();
        let err = call_guarded(&lua, &runaway, ()).unwrap_err();
        assert!(
            err.contains("callback exceeded the instruction budget"),
            "{err}"
        );
    }

    #[test]
    fn budget_error_has_app_position() {
        let err = run(LOOP).unwrap_err();
        assert!(
            err.starts_with("index.lua:1: callback exceeded the instruction budget"),
            "{err}"
        );
    }

    #[test]
    fn traceback_starts_at_the_failing_frame() {
        let err = run("local t = nil; return t.x").unwrap_err();
        assert!(
            err.ends_with("(local 't')\nstack traceback:\n\tindex.lua:1: in function 'on_load'"),
            "{err}"
        );
    }

    #[test]
    fn no_coroutine_carries_the_exhausted_hook_into_the_next_call() {
        let lua = new_lua();
        set_budget(&lua, 1_000_000);
        let run = |code: &str| {
            let f = load_chunk(&lua, code.as_bytes(), "@index.lua").unwrap();
            call_guarded(&lua, &f, ())
        };
        // 600 000 instructions per resume: an every-instruction hook would count them 10 000 times over
        let spin = "for i = 1, 300000 do end";
        run(&format!(
            "kept = coroutine.wrap(function() while true do {spin} coroutine.yield() end end) kept()"
        ))
        .unwrap();
        // a coroutine trips the budget itself and swallows the abort; it cannot survive that
        let err = run(&format!(
            "tripped = coroutine.create(function() while true do pcall(function() {LOOP} end) coroutine.yield() end end) \
             coroutine.resume(tripped)"
        ))
        .unwrap_err();
        assert!(
            err.contains("callback exceeded the instruction budget"),
            "{err}"
        );
        // the main thread itself switched to the every-instruction hook
        run(LOOP).unwrap_err();
        for _ in 0..3 {
            run("kept()").unwrap();
            let dead = run("return coroutine.status(tripped)").unwrap();
            assert_eq!(
                dead.into_iter().next().unwrap().to_string().unwrap(),
                "dead"
            );
        }
    }

    #[test]
    fn exhaustion_in_a_host_function_calling_back_into_lua_still_aborts() {
        // a Rust method that calls a Lua callback flattens its error to text; the exhausted budget must survive that
        let lua = new_lua();
        set_budget(&lua, 1_000_000);
        let host = lua
            .create_function(|lua, f: Function| {
                let r = call_guarded(lua, &f, ());
                Ok(r.err())
            })
            .unwrap();
        lua.globals().set("host", host).unwrap();
        for code in [
            format!("pcall(host, function() {LOOP} end) return 'swallowed'"),
            format!("host(function() {LOOP} end) for i = 1, 10 do end return 'continued'"),
        ] {
            let f = load_chunk(&lua, code.as_bytes(), "@index.lua").unwrap();
            let err = call_guarded(&lua, &f, ()).unwrap_err();
            assert!(
                err.contains("callback exceeded the instruction budget"),
                "{code}: {err}"
            );
        }
    }

    #[test]
    fn budget_resets_per_call() {
        let lua = new_lua();
        set_budget(&lua, 1_000_000);
        let f = load_chunk(&lua, b"for i = 1, 200000 do end", "@index.lua").unwrap();
        for _ in 0..5 {
            call_guarded(&lua, &f, ()).unwrap();
        }
    }

    #[test]
    fn load_cannot_fake_a_host_chunk() {
        let err = run(&format!("load('{LOOP}', '{HOST_SRC}')()")).unwrap_err();
        assert!(
            err.contains("callback exceeded the instruction budget"),
            "{err}"
        );
    }

    #[test]
    fn sandbox_globals() {
        let lua = new_lua();
        let ok: bool = lua
            .load(
                "return io == nil and os == nil and debug == nil and dofile == nil and loadfile == nil \
                 and collectgarbage == nil and print == nil and package.loadlib == nil and #package.searchers == 1",
            )
            .set_name("@t.lua")
            .eval()
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn load_is_text_only() {
        let lua = new_lua();
        let (f, err): (Option<Function>, String) = lua
            .load("return load(string.dump(function() end))")
            .set_name("@t.lua")
            .eval()
            .unwrap_or_else(|_| (None, "string.dump failed".into()));
        assert!(f.is_none(), "{err}");
        let err = lua
            .load("load(print)")
            .set_name("@t.lua")
            .exec()
            .unwrap_err();
        assert!(
            error_text(&err).starts_with("t.lua:1: bad argument #1 to 'load' (only string chunks"),
            "{err}"
        );
    }

    #[test]
    fn require_searches_the_app_folder_only() {
        let lua = new_lua();
        let files = AppFiles(
            [
                ("lib/x.lua".to_string(), b"return {v = 7}".to_vec()),
                ("lib/bad.lua".into(), b"return (".to_vec()),
            ]
            .into(),
        );
        install_require(&lua, Rc::new(files)).unwrap();
        let v: i64 = lua
            .load("return require('lib.x').v")
            .set_name("@t.lua")
            .eval()
            .unwrap();
        assert_eq!(v, 7);
        let err = error_text(
            &lua.load("require('../x')")
                .set_name("@t.lua")
                .exec()
                .unwrap_err(),
        );
        assert!(
            err.starts_with(
                "t.lua:1: module '../x' not found:\n\t\n\tno file '///x.lua' in the app folder"
            ),
            "{err}"
        );
        let err = error_text(
            &lua.load("require('lib.bad')")
                .set_name("@t.lua")
                .exec()
                .unwrap_err(),
        );
        assert!(
            err.starts_with(
                "error loading module 'lib.bad' from file 'lib/bad.lua':\n\tlib/bad.lua:1:"
            ),
            "{err}"
        );
    }
}
