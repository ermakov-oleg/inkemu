//! Tables whose methods only run when called with a colon on the object itself (0.1 `make_object`).
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::lua::error_text;

/// A method body: receives the arguments after `self`.
pub type Method = Rc<dyn Fn(&Lua, MultiValue) -> mlua::Result<MultiValue>>;

pub fn method(f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + 'static) -> Method {
    Rc::new(f)
}

/// A new object called `name` (`ctx.<name>` in messages; `ctx` itself is named "ctx").
pub fn new_object(lua: &Lua, name: &str, methods: &[(&str, Method)]) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let register: Function = lua.named_registry_value("inkemu.register_object")?;
    register.call::<()>((&t, name))?;
    add_methods(lua, &t, name, methods)?;
    Ok(t)
}

/// Adds colon-checked methods to an object made by `new_object`.
pub fn add_methods(
    lua: &Lua,
    t: &Table,
    name: &str,
    methods: &[(&str, Method)],
) -> mlua::Result<()> {
    let wrap: Function = lua.named_registry_value("inkemu.object_method")?;
    for (fn_name, m) in methods {
        let m = m.clone();
        let body = lua.create_function(move |lua, args: MultiValue| {
            let mut out = MultiValue::new();
            match m(lua, args) {
                Ok(r) => {
                    out.extend(r);
                    out.push_front(Value::Boolean(true));
                }
                Err(e) => {
                    out.push_front(Value::String(lua.create_string(error_text(&e))?));
                    out.push_front(Value::Boolean(false));
                }
            }
            Ok(out)
        })?;
        t.raw_set(*fn_name, wrap.call::<Function>((name, *fn_name, body))?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::{call_guarded, load_chunk, new_lua};

    #[test]
    fn rust_errors_carry_the_app_position_and_dot_calls_name_the_method() {
        let lua = new_lua();
        let boom = method(|_, _| Err(mlua::Error::runtime("kaput")));
        let ok = method(|_, args| Ok(args));
        let obj = new_object(&lua, "clock", &[("boom", boom), ("echo", ok)]).unwrap();
        lua.globals().set("obj", obj).unwrap();
        let run = |code: &str| {
            let f = load_chunk(&lua, code.as_bytes(), "@index.lua").unwrap();
            call_guarded(&lua, &f, ()).map(|r| r.len())
        };
        assert_eq!(run("return obj:echo(1, 2, nil)"), Ok(3));
        let err = run("\nobj:boom()").unwrap_err();
        assert!(err.starts_with("index.lua:2: kaput"), "{err}");
        let err = run("obj.echo()").unwrap_err();
        assert!(
            err.starts_with(
                "index.lua:1: bad argument #2 to 'echo' (call ctx.clock:echo with a colon)"
            ),
            "{err}"
        );
    }
}
