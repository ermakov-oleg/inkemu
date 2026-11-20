use mlua::Lua;

/// The only way to create a Lua state: checks that Lua was built with the device number types.
pub fn new_lua() -> Lua {
    let lua = Lua::new();
    let ok: bool = lua
        .load("return math.maxinteger == 2147483647 and string.format('%.17g', 0.1) == '0.10000000149011612'")
        .eval()
        .expect("number type check failed to run");
    assert!(
        ok,
        "Lua was built with the wrong number types: run scripts/vendor-lua.sh"
    );
    lua
}
