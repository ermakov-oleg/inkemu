use inkcore::lua::new_lua;

#[test]
fn maxinteger_is_int32() {
    let lua = new_lua();
    let v: i64 = lua.load("return math.maxinteger").eval().unwrap();
    assert_eq!(v, 2147483647);
}

#[test]
fn string_format_rejects_big_float() {
    let lua = new_lua();
    let err = lua
        .load("return string.format('%08x', 4294967295 // 1)")
        .eval::<String>()
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("number has no integer representation"),
        "{err}"
    );
}

#[test]
fn float_width_matches_mode() {
    let lua = new_lua();
    let s: String = lua
        .load("return string.format('%.17g', 0.1)")
        .eval()
        .unwrap();
    assert_eq!(s, "0.10000000149011612");
}
