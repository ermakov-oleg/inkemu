from pathlib import Path

import pytest
from conftest import IMPL

ROOT = Path(__file__).resolve().parent.parent
HELLO = ROOT / "examples" / "hello"


def test_hello_tap_increments(emu_factory):
    emu = emu_factory(HELLO)
    assert emu.has_text("Taps: 0")
    emu.tap(240, 600)
    assert emu.has_text("Taps: 1")


def test_hello_state_survives_restart(emu_factory):
    emu = emu_factory(HELLO)
    emu.tap(240, 600)
    emu.restart()
    assert emu.has_text("Taps: 1")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 6")
def test_hello_clock_redraws_after_minute(emu_factory):
    emu = emu_factory(HELLO)
    assert emu.has_text("09:00")
    emu.wait("61s")
    assert emu.has_text("09:01")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 6")
def test_hello_shows_battery(emu_factory):
    emu = emu_factory(HELLO)
    assert emu.has_text("Battery 87%")


def test_example_is_neutral_for_publishing(emu_factory):
    assert emu_factory(HELLO).app_id == "hello"
    assert "<you>" not in (ROOT / "README.md").read_text(encoding="utf-8")
