import hashlib
from pathlib import Path

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"
# the same value is pinned in crates/inkcore/examples/hello_hash.rs, which CI also runs under wasm
HELLO_HASH = "077f5578ec11f69429c25e4db0e2b5c8b016abd240a6a270143021d5fceae4c0"


def test_hello_frame_hash_matches_the_rust_example(emu_factory):
    emu = emu_factory(HELLO, fresh=True, start_time="2026-11-20T09:00:00")
    emu.tap_text("Tap me")
    emu.wait("61s")
    assert hashlib.sha256(emu.raw_frame().tobytes()).hexdigest() == HELLO_HASH
