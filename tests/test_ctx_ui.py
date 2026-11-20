from io import BytesIO

import pytest
from PIL import Image

DRAW = "function on_draw(c, g) g:clear(0) %s end\n"


def jpeg_bytes(w=4, h=3, color=0) -> bytes:
    buf = BytesIO()
    Image.new("L", (w, h), color).save(buf, "JPEG")
    return buf.getvalue()


def black(img: Image.Image, x: int, y: int) -> bool:
    return img.getpixel((x, y)) == 0


def test_input_text_ok(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "got = ''\nfunction on_load(c) c.input:text({title='Name', text='a'}, function(c, s) got = s; c:invalidate() end) end\n"
            "function on_draw(c, g) g:clear(0); g:text(0, 0, 'got ' .. got) end\n"
        )
    )
    assert emu.pending == {"kind": "text", "title": "Name", "text": "a"}
    emu.type_text("Bob")
    assert emu.pending is None
    assert emu.has_text("got Bob")


def test_input_text_cancel_does_not_call_back(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "got = 'none'\nfunction on_load(c) c.input:text({title='N', text=''}, function(c, s) got = s end) end\n"
            "function on_draw(c, g) g:text(0, 0, got) end\n"
        )
    )
    emu.answer(False)
    assert emu.pending is None and emu.has_text("none")


def test_input_edit_feeds_chars(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "changes, ended = {}, nil\n"
            "function on_load(c)\n"
            "  ed = c.input:edit({text='', on_change=function(c, s) changes[#changes + 1] = s end,\n"
            "    on_end=function(c, s, why) ended = s .. ':' .. why; c:invalidate() end})\n"
            "end\n"
            "function on_draw(c, g) g:clear(0); g:text(0, 0, table.concat(changes, ',') .. ' ' .. tostring(ended)) end\n"
        )
    )
    assert emu.pending["kind"] == "edit"
    emu.type_text("abc")
    assert emu.pending is None
    assert emu.has_text("a,ab,abc abc:enter")


def test_input_edit_object(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c)\n"
            "  ed = c.input:edit({text='x', on_change=function() end, on_end=function() end})\n"
            "  ed:set_text('hello')\n  seen = ed.text\nend\n"
            "function on_draw(c, g) g:text(0, 0, seen) end\n"
        )
    )
    assert emu.pending["text"] == "hello" and emu.has_text("hello")
    emu._call("on_load", emu._ctx)  # a second edit replaces the first
    assert emu.pending["kind"] == "edit"
    emu.L.execute("ed:close()")
    assert emu.pending is None


def test_confirm_yes_no(make_app, emu_factory):
    app = make_app(
        "res = 'none'\nfunction on_load(c) c.dialog:confirm({title='T', text='Sure?'}, function(c, ok) res = tostring(ok); c:invalidate() end) end\n"
        "function on_draw(c, g) g:clear(0); g:text(0, 0, 'res ' .. res) end\n"
    )
    emu = emu_factory(app)
    assert emu.pending == {"kind": "confirm", "title": "T", "text": "Sure?"}
    emu.answer(False)
    assert emu.has_text("res false") and emu.pending is None
    emu = emu_factory(app)
    emu.answer(True)
    assert emu.has_text("res true")


def test_on_pending_hook(make_app, emu_factory):
    seen = []
    app = make_app(
        "function on_load(c) end\nfunction on_input(c, e) if e.phase == 'up' and e.key == 'ok' then c.dialog:confirm({title='T', text='x'}, function() end) end return e.key ~= 'back' end\n"
    )
    emu = emu_factory(app)
    emu.on_pending = seen.append
    emu.key("ok")
    assert seen == [{"kind": "confirm", "title": "T", "text": "x"}]
    emu.answer(False)
    emu.key("back")
    assert seen[-1]["kind"] == "leave"


def test_answer_or_type_text_without_pending(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) end\n"))
    with pytest.raises(RuntimeError):
        emu.answer(True)
    with pytest.raises(RuntimeError):
        emu.type_text("x")


def test_leave_answer_yes_calls_on_leave(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) end\nfunction on_leave(c) c.storage:write('left', 'yes') end\n"))
    emu.key("back")
    assert emu.pending["kind"] == "leave"
    emu.answer(True)
    assert emu.pending is None and emu.status()["running"] is False
    assert (emu.state_dir / "files" / "left").read_text() == "yes"


def test_leave_answer_no_keeps_running(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) end\n"))
    emu.key("back")
    emu.answer(False)
    assert emu.pending is None and emu.status()["running"] is True


def test_toast_visible_then_gone(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) c.dialog:toast('Saved') end\n" + DRAW % ""))
    assert emu.has_text("Saved")
    ys = [t["y"] for t in emu.texts() if "Saved" in t["text"]]
    assert ys and ys[0] > 600
    assert any(black(emu.shot(), 16 + dx, y) for dx in (0, 1) for y in range(ys[0] - 20, ys[0]))
    emu.wait("3s")
    assert not emu.has_text("Saved")
    assert emu.toasts == ["Saved"]
    assert not any(black(emu.shot(), 20, y) for y in range(700, 790))


def test_toast_lifetime_is_virtual_2s(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) c.dialog:toast('Hi') end\n" + DRAW % ""))
    emu.wait(1900)
    assert emu.has_text("Hi")
    emu.wait(200)
    assert not emu.has_text("Hi")


def test_image_decode_jpeg_and_draw(make_app, emu_factory, tmp_path):
    app = make_app(
        "function on_load(c)\n  img = c.image:decode(c.package:read('b.jpg'))\n  w, h = img.width, img.height\nend\n"
        "function on_draw(c, g) g:clear(0); g:image(img, 10, 10); g:text(0, 100, w .. 'x' .. h) end\n"
    )
    (app / "data").mkdir()
    (app / "data" / "b.jpg").write_bytes(jpeg_bytes(4, 3))
    emu = emu_factory(app)
    assert emu.has_text("4x3")
    shot = emu.shot()
    assert black(shot, 10, 10) and black(shot, 13, 12) and not black(shot, 14, 10)


def test_package_image(make_app, emu_factory):
    app = make_app(
        "function on_load(c)\n  img, err = c.package:image('b.jpg')\n  miss, merr = c.package:image('none.jpg')\nend\n"
        "function on_draw(c, g) g:clear(0); g:image(img, 0, 0); g:text(0, 100, tostring(img.width) .. ' ' .. tostring(merr)) end\n"
    )
    (app / "data").mkdir()
    (app / "data" / "b.jpg").write_bytes(jpeg_bytes(5, 5))
    emu = emu_factory(app)
    assert emu.has_text("5 not_found") and black(emu.shot(), 4, 4)


def test_png_decode_warns(make_app, emu_factory):
    buf = BytesIO()
    Image.new("L", (2, 2), 0).save(buf, "PNG")
    app = make_app(
        "function on_load(c) img = c.image:decode(c.package:read('b.png')) end\n"
        "function on_draw(c, g) g:text(0, 0, tostring(img.width)) end\n"
    )
    (app / "data").mkdir()
    (app / "data" / "b.png").write_bytes(buf.getvalue())
    emu = emu_factory(app)
    assert emu.has_text("2")
    assert any("PNG" in w for w in emu.warnings) and any("PNG" in line for line in emu.log)


def test_decode_garbage(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) img, err = c.image:decode('not an image') end\n"
            "function on_draw(c, g) g:text(0, 0, tostring(img) .. ' ' .. tostring(err)) end\n"
        )
    )
    assert emu.has_text("nil decode_failed")


@pytest.mark.allow_errors
def test_released_image_draw_is_an_error(make_app, emu_factory):
    app = make_app(
        "function on_load(c) img = c.image:decode(c.package:read('b.jpg')); img:release() end\n"
        "function on_draw(c, g) g:image(img, 0, 0) end\n"
    )
    (app / "data").mkdir()
    (app / "data" / "b.jpg").write_bytes(jpeg_bytes())
    emu = emu_factory(app)
    assert emu.errors and "released" in emu.errors[0]
    assert not emu._images


def test_qr_encode_draws(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) qr = c.qr:encode('hi') end\n"
            "function on_draw(c, g) g:clear(0); g:image(qr, 0, 0, {w=99, h=99}) end\n"
        )
    )
    shot = emu.shot()
    # 21 modules scaled to 99 px: outer ring black, inner ring white, 3x3 core black, separator white
    assert all(black(shot, x, 2) for x in range(0, 31)) and black(shot, 2, 30)
    assert not black(shot, 7, 7) and not black(shot, 24, 7)
    assert black(shot, 16, 16) and not black(shot, 36, 2)


def test_qr_is_standard_size_without_border(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) qr = c.qr:encode('hi'); w, h = qr.width, qr.height end\n"
            "function on_draw(c, g) g:text(0, 0, w .. 'x' .. h) end\n"
        )
    )
    assert emu.has_text("21x21")


def test_qr_too_long(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) q, err = c.qr:encode(string.rep('x', 5000)) end\n"
            "function on_draw(c, g) g:text(0, 0, tostring(q) .. ' ' .. tostring(err)) end\n"
        )
    )
    assert emu.has_text("nil invalid_argument")


def test_layer_draw(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c)\n  layer = c.layers:create(40, 30)\n"
            "  layer:draw(function(g) g:rect(0, 0, 10, 10, {fill=true}) end)\nend\n"
            "function on_draw(c, g) g:clear(0); g:image(layer, 100, 100); g:text(0, 0, layer.width .. 'x' .. layer.height) end\n"
        )
    )
    shot = emu.shot()
    assert black(shot, 105, 105) and not black(shot, 115, 105) and not black(shot, 105, 125)
    assert emu.has_text("40x30")


def test_layer_accumulates_and_released(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c)\n  layer = c.layers:create(40, 30)\n"
            "  layer:draw(function(g) g:rect(0, 0, 10, 10, {fill=true}) end)\n"
            "  layer:draw(function(g) g:rect(20, 0, 10, 10, {fill=true}) end)\n"
            "  other = c.layers:create(10, 10)\n  other:draw(function(g) g:image(layer, 0, 0) end)\nend\n"
            "function on_draw(c, g) g:clear(0); g:image(layer, 100, 100) end\n"
        )
    )
    shot = emu.shot()
    assert black(shot, 105, 105) and black(shot, 125, 105)


@pytest.mark.allow_errors
def test_released_layer_draw_is_an_error(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) layer = c.layers:create(10, 10); layer:release(); layer:draw(function(g) end) end\n"
        )
    )
    assert emu.errors and "released" in emu.errors[0]


@pytest.mark.allow_errors
def test_dot_call_is_an_error(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) c.dialog.toast('x') end\n"))
    assert emu.errors and "colon" in emu.errors[0]


def test_edit_close_inside_on_change_stops_feeding(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "changes, ended = {}, 'no'\n"
            "function on_load(c)\n"
            "  ed = c.input:edit({text='', on_change=function(c, s) changes[#changes + 1] = s; c:invalidate()\n"
            "      if #s == 2 then ed:close() end end,\n"
            "    on_end=function(c, s, why) ended = why; c:invalidate() end})\n"
            "end\n"
            "function on_draw(c, g) g:clear(0); g:text(0, 0, table.concat(changes, ',') .. ' ' .. ended) end\n"
        )
    )
    emu.type_text("abcd")
    assert emu.pending is None and emu.has_text("a,ab no")


def test_no_ttf_font_warns_once(make_app, emu_factory, monkeypatch):
    from inkemu import fonts

    monkeypatch.setattr(fonts, "FONT_FILES", [])
    emu = emu_factory(make_app("function on_draw(c, g) g:text(0, 0, 'Привет') end\n"))
    emu.reload()
    assert len([w for w in emu.warnings if "no TrueType font" in w and "boxes" in w]) == 1
