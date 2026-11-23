//! The frame hash that native, wasm and the Python `Emu` must agree on: `examples/hello` started fresh at
//! 2026-11-20T09:00:00, after `tap-text 'Tap me'` and `wait 61s`; sha256 of the raw frame in PIL mode "1".

use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};
use sha2::{Digest, Sha256};

const START_LOCAL_MS: i64 = 1_795_165_200_000; // 2026-11-20T09:00:00

fn hello_hash() -> String {
    let file = |name: &str, text: &str| (name.to_string(), text.as_bytes().to_vec());
    let app = AppFiles(BTreeMap::from([
        file(
            "manifest.json",
            include_str!("../../../examples/hello/manifest.json"),
        ),
        file(
            "index.lua",
            include_str!("../../../examples/hello/index.lua"),
        ),
        file(
            "lockscreen.lua",
            include_str!("../../../examples/hello/lockscreen.lua"),
        ),
    ]));
    let cfg = Config {
        start_local_ms: START_LOCAL_MS,
        ..Config::default()
    };
    let mut core = Core::new(app, cfg, Persisted::default());
    core.tap_text("Tap me").unwrap();
    core.wait(61_000).unwrap();
    assert!(core.errors().is_empty(), "{:?}", core.errors());
    Sha256::digest(core.raw_frame().packed())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn main() {
    println!("{}", hello_hash());
}

#[cfg(test)]
mod tests {
    // tests/test_parity.py pins the same value for the Python `Emu`
    const PINNED: &str = "382aeb4feb77bc990eb6ba982afff505f55454c768f3dce4518d7a6725b46e67";

    #[test]
    fn hello_frame_hash_is_pinned() {
        assert_eq!(super::hello_hash(), PINNED);
    }
}
