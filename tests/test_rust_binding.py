import re
from datetime import datetime

import pytest
from conftest import IMPL

pytestmark = pytest.mark.skipif(IMPL != "rust", reason="pins the Rust binding only")


@pytest.fixture
def emu(make_app, emu_factory):
    return emu_factory(make_app(""))


def test_lua_eval_round_trips_a_nested_table(emu):
    got = emu.lua_eval("return {name = 'a', list = {1, 2, {x = true}}, nested = {k = {v = 'deep'}}}")
    assert got == {"name": "a", "list": [1, 2, {"x": True}], "nested": {"k": {"v": "deep"}}}


def test_raw_frame_is_a_480x800_image(emu):
    img = emu.raw_frame()
    assert img.size == (480, 800) and img.mode == "1"


def test_clock_now_is_the_start_time(emu):
    assert emu.clock.now() == datetime(2026, 11, 20, 9, 0, 0)


def test_log_line_is_stamped(emu):
    emu.log_line("marker")
    assert re.fullmatch(r"\d\d:\d\d:\d\d\.\d{3} marker", emu.log[-1]), emu.log[-1]


def test_not_yet_implemented_method_raises(emu):
    with pytest.raises(NotImplementedError):
        emu.lock()
