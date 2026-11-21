//! `ctx:save_state`, `ctx:defer_full_refresh`, `ctx.api` and `ctx.state` (0.1 `ctx_basic.install`,
//! `load_state`, `save_state`).
use mlua::{Lua, Table, Value};
use serde_json::Value as J;

use super::convert::{big_int_warning, dump, json_to_lua, lua_to_json};
use super::object::{add_methods, method};
use super::{arg, check_arg, nil, py_int, type_name};
use crate::emu::{with, Host, Shared};

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let defer_full_refresh = {
        let st = st.clone();
        method(move |_, args| {
            let a = args.into_vec();
            check_arg("defer_full_refresh", 1, arg(&a, 0), "number")?;
            let n = py_int(arg(&a, 0))?;
            with(&st, |s| s.full_refresh_deferred = n);
            Ok(nil())
        })
    };
    let save_state = {
        let st = st.clone();
        method(move |lua, _| {
            Host { lua, st: &st }.save_state();
            Ok(nil())
        })
    };
    add_methods(
        lua,
        ctx,
        "ctx",
        &[
            ("defer_full_refresh", defer_full_refresh),
            ("save_state", save_state),
        ],
    )?;
    let ((major, minor), saved, label) = with(st, |s| {
        (
            s.cfg.firmware_api,
            s.persisted.state_json.clone(),
            s.cfg.state_label.clone(),
        )
    });
    let api = lua.create_table()?;
    api.raw_set("major", major)?;
    api.raw_set("minor", minor)?;
    ctx.raw_set("api", api)?;
    let mut warnings = vec![];
    let state = load_state(lua, saved.as_deref(), &label, &mut warnings)?;
    with(st, |s| warnings.iter().for_each(|w| s.warn(w)));
    ctx.raw_set("state", state)
}

/// `ctx.state` from the saved JSON; an unreadable or non-table file starts empty with a warning.
fn load_state(
    lua: &Lua,
    saved: Option<&[u8]>,
    label: &str,
    warnings: &mut Vec<String>,
) -> mlua::Result<Value> {
    let data = match saved.map(serde_json::from_slice::<J>) {
        None => None,
        Some(Err(e)) => {
            warnings.push(format!(
                "{label}: unreadable state ({e}), starting with an empty ctx.state"
            ));
            None
        }
        Some(Ok(v @ (J::Object(_) | J::Array(_)))) => Some(v),
        Some(Ok(_)) => {
            warnings.push(format!(
                "{label}: state is not a table, starting with an empty ctx.state"
            ));
            None
        }
    };
    let Some(data) = data else {
        return Ok(Value::Table(lua.create_table()?));
    };
    let mut big = None;
    let v = json_to_lua(lua, &data, None, &mut |n| {
        big.get_or_insert_with(|| big_int_warning(n));
    })?;
    warnings.extend(big);
    Ok(v)
}

/// Serialises `ctx.state` into the persisted data (0.1 `save_state`); the lock screen never overwrites it.
pub(crate) fn save_state(h: &Host) {
    let Some(ctx) = h.ctx() else { return };
    if with(h.st, |s| s.locked) {
        return;
    }
    let state = ctx.raw_get::<Value>("state").unwrap_or(Value::Nil);
    let mut warnings = vec![];
    let bytes = match lua_to_json(&state, "state", None, &mut |w| warnings.push(w)) {
        Some(J::Null) => Some(b"{}".to_vec()),
        Some(d @ (J::Object(_) | J::Array(_))) => Some(dump(&d, false).into_bytes()),
        _ => None,
    };
    with(h.st, |s| {
        warnings.iter().for_each(|w| s.warn(w));
        let Some(bytes) = bytes else {
            let ty = type_name(&state);
            s.warn(&format!("ctx.state is a {ty}, not a table: not saved"));
            return;
        };
        if s.persisted.state_json.as_deref() != Some(bytes.as_slice()) {
            s.persisted.state_json = Some(bytes);
            s.persist_changed = true;
        }
    });
}
