//! `ctx.ui` (§16): the UI library, written in Lua and run in the app's state with a private environment.
use mlua::{Function, Lua, Table, Value};
use serde_json::Value as J;

use super::object::new_object;
use crate::app::AppFiles;

/// The library's files, run in this order in one environment: a later file sees what the earlier ones define.
const FILES: &[(&str, &str)] = &[
    ("ui.lua", include_str!("ui.lua")),
    ("fields.lua", include_str!("fields.lua")),
    ("frame.lua", include_str!("frame.lua")),
    ("focus.lua", include_str!("focus.lua")),
    ("nav.lua", include_str!("nav.lua")),
    ("events.lua", include_str!("events.lua")),
    ("paint.lua", include_str!("paint.lua")),
    ("controls.lua", include_str!("controls.lua")),
    ("composite.lua", include_str!("composite.lua")),
    ("layout_tree.lua", include_str!("layout_tree.lua")),
    ("layout_flex.lua", include_str!("layout_flex.lua")),
    ("layout.lua", include_str!("layout.lua")),
];

/// The standard globals the library may use; the app's own globals stay out of its reach.
const GLOBALS: [&str; 14] = [
    "error",
    "getmetatable",
    "ipairs",
    "next",
    "pairs",
    "pcall",
    "rawequal",
    "rawget",
    "rawset",
    "select",
    "setmetatable",
    "tonumber",
    "tostring",
    "type",
];

/// Copied rather than shared, so an app that patches `string.sub` or `table.insert` cannot step into the library.
const LIBRARIES: [&str; 4] = ["math", "string", "table", "utf8"];

// `(true, ...)` or `(false, message)` for `inkemu.object_method`; budget-exempt host code, so nothing else goes here
const CREATE: &str =
    "local pcall, create = pcall, ... return function(...) return pcall(create, ...) end";

pub(super) fn install(lua: &Lua, ctx: &Table, app: &AppFiles) -> mlua::Result<()> {
    let env = load(lua, FILES, host(lua, ctx, app)?)?;
    ctx.raw_set("ui", namespace(lua, &env)?)
}

/// What the library learns from the host: the package's API minor, the touchscreen, `ctx.fonts` and `ctx.input`.
fn host(lua: &Lua, ctx: &Table, app: &AppFiles) -> mlua::Result<Table> {
    let host = lua.create_table()?;
    host.raw_set("api_minor", api_minor(app))?;
    // the X4 Pro has a touchscreen, as `ctx.input.pointer` says
    host.raw_set("pointer", true)?;
    host.raw_set("fonts", ctx.raw_get::<Value>("fonts")?)?;
    host.raw_set("input", ctx.raw_get::<Value>("input")?)?;
    Ok(host)
}

/// Runs `files` in a fresh environment holding [`GLOBALS`], copies of [`LIBRARIES`] and `host`.
fn load(lua: &Lua, files: &[(&str, &str)], host: Table) -> mlua::Result<Table> {
    let globals = lua.globals();
    let env = lua.create_table()?;
    for name in GLOBALS {
        env.raw_set(name, globals.raw_get::<Value>(name)?)?;
    }
    for name in LIBRARIES {
        let copy = lua.create_table()?;
        globals
            .raw_get::<Table>(name)?
            .for_each(|k: Value, v: Value| copy.raw_set(k, v))?;
        env.raw_set(name, copy)?;
    }
    env.raw_set("host", host)?;
    for (name, src) in files {
        // not `=[host]`: the library's instructions count against the app's callback budget, as on the device
        lua.load(*src)
            .set_name(format!("=ctx.ui/{name}"))
            .set_environment(env.clone())
            .exec()?;
    }
    Ok(env)
}

/// `ctx.ui`: an object whose only member is the colon-checked `create`.
fn namespace(lua: &Lua, env: &Table) -> mlua::Result<Table> {
    let body: Function = lua
        .load(CREATE)
        .set_name("=[host]")
        .call(env.raw_get::<Function>("create")?)?;
    let ui = new_object(lua, "ui", &[])?;
    let wrap: Function = lua.named_registry_value("inkemu.object_method")?;
    ui.raw_set("create", wrap.call::<Function>(("ui", "create", body))?)?;
    Ok(ui)
}

/// The `<minor>` of the manifest's `api` "2.<minor>": 0 for an older major. Boot has already refused a manifest
/// whose `api` does not parse or is newer than the firmware's.
fn api_minor(app: &AppFiles) -> i32 {
    let api = app
        .get("manifest.json")
        .and_then(|raw| serde_json::from_slice::<J>(raw).ok())
        .and_then(|m| m.get("api").cloned());
    let text = match api {
        None => "0.8".to_string(),
        Some(J::String(s)) => s,
        Some(J::Number(n)) => n.to_string(),
        Some(_) => return 0,
    };
    let Some((major, minor)) = text.trim().split_once('.') else {
        return 0;
    };
    match major.parse::<u32>() {
        Ok(2) => minor
            .parse::<u32>()
            .map_or(0, |m| i32::try_from(m).unwrap_or(i32::MAX)),
        Ok(m) if m > 2 => i32::MAX,
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
