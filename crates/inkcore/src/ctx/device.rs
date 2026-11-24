//! `ctx.device` (0.1 `_install_device`), with the fields of firmware 26112001.
use mlua::{IntoLuaMulti, Lua, Table};
use sha2::{Digest, Sha256};

use super::object::{method, new_object};
use super::utils::hex;
use crate::emu::{with, Shared};

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let battery = {
        let st = st.clone();
        method(move |lua, _| {
            let b = with(&st, |s| s.cfg.battery.clone());
            let t = lua.create_table()?;
            t.raw_set("percent", b.percent)?;
            t.raw_set("charging", b.charging)?;
            t.raw_set("voltage_mv", b.voltage_mv)?;
            t.raw_set("temperature_c", b.temperature_c)?;
            t.into_lua_multi(lua)
        })
    };
    let app_id = with(st, |s| s.app_id.clone());
    let device = new_object(lua, "device", &[("battery", battery)])?;
    device.raw_set("firmware", "XT Licorice 26112001")?;
    device.raw_set("model", "x4_pro")?;
    device.raw_set("name", "book")?;
    device.raw_set("lock_timer", true)?;
    let id = hex(&Sha256::digest(format!("inkemu:{app_id}")));
    device.raw_set("id_for_app", &id[..16])?;
    ctx.raw_set("device", device)
}
