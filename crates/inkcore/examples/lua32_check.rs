use inkcore::lua::new_lua;

fn main() {
    let lua = new_lua();
    let max: i64 = lua.load("return math.maxinteger").eval().unwrap();
    assert_eq!(max, 2147483647);
    let err = lua
        .load("return string.format('%08x', 4294967295 // 1)")
        .eval::<String>()
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("number has no integer representation"),
        "{err}"
    );
    let s: String = lua
        .load("return string.format('%.17g', 0.1)")
        .eval()
        .unwrap();
    assert_eq!(s, "0.10000000149011612");
    println!("ok");
}
