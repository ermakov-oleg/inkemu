"""UI-ish ctx members: dialogs, text input, toast, image decoding, QR codes, offscreen layers."""
from io import BytesIO
from typing import TYPE_CHECKING, Any

import segno
from lupa import lua55
from PIL import Image

from .ctx_basic import _bytes_arg, _binary_object, check_arg
from .draw import Canvas
from .lua_env import make_object

if TYPE_CHECKING:
    from .emu import Emu

MAX_LAYER_SIDE = 2048


def _opt_str(opts: Any, key: str) -> str:
    v = opts[key]
    return "" if v is None else str(v)


def _flatten(img: Image.Image) -> Image.Image:
    """Grayscale copy, transparency composited over white."""
    if img.mode in ("RGBA", "LA", "PA") or "transparency" in img.info:
        rgba = img.convert("RGBA")
        bg = Image.new("RGBA", rgba.size, (255, 255, 255, 255))
        return Image.alpha_composite(bg, rgba).convert("L")
    return img.convert("L")


def install(emu: "Emu") -> None:
    L, ctx = emu.L, emu._ctx
    next_id = [0]

    def new_image(img: Image.Image, methods: dict | None = None, name: str = "image"):
        next_id[0] += 1
        iid = next_id[0]
        emu._images[iid] = img

        def release(*_):
            emu._images.pop(iid, None)

        obj = make_object(L, name, {"release": release, **(methods or {})},
                          fields={"width": img.width, "height": img.height, "_id": iid})
        return obj, iid

    def resolve(obj) -> Image.Image | None:
        if lua55.lua_type(obj) != "table":
            return None
        iid = obj["_id"]
        if iid is None:
            return None
        img = emu._images.get(iid)
        if img is None:
            raise ValueError("g:image: the image or layer was released")
        return img

    emu.canvas.image_source = resolve

    # ---- dialogs and input -------------------------------------------------------------------

    def call_back(fn, *args, where: str):
        emu._protected(fn, ctx, *args, where=where)

    def input_text(opts=None, fn=None, *_):
        check_arg("text", 1, opts, "table")
        check_arg("text", 2, fn, "function")
        default = _opt_str(opts, "text")

        def done(text: str):
            emu._clear_pending()
            call_back(fn, text, where="ctx.input:text callback")

        emu._set_pending("text", _opt_str(opts, "title"), default,
                         answer=lambda ok: done(default) if ok else None, type_text=done)

    def input_edit(opts=None, *_):
        check_arg("edit", 1, opts, "table")
        on_change, on_end = opts["on_change"], opts["on_end"]
        st = {"text": _opt_str(opts, "text")}
        handle: dict = {}

        def sync(text: str):
            st["text"] = text
            handle["obj"]["text"] = text
            if emu._pending_h is handle["h"]:
                emu.pending["text"] = text

        def finish(reason: str):
            if emu._pending_h is handle["h"]:
                emu._clear_pending()
            if on_end is not None:
                call_back(on_end, st["text"], reason, where="ctx.input:edit on_end")

        def type_text(s: str):
            for ch in s:
                sync(st["text"] + ch)
                if on_change is not None:
                    call_back(on_change, st["text"], where="ctx.input:edit on_change")
            finish("enter")

        def set_text(s=None, *_):
            check_arg("set_text", 1, s, "string")
            sync(s)

        def close(*_):
            if emu._pending_h is handle["h"]:
                emu._clear_pending()

        handle["obj"] = make_object(L, "edit", {"set_text": set_text, "close": close}, fields={"text": st["text"]})
        handle["h"] = emu._set_pending("edit", _opt_str(opts, "title"), st["text"],
                                       answer=lambda ok: finish("enter" if ok else "cancel"), type_text=type_text)
        return handle["obj"]

    def confirm(opts=None, fn=None, *_):
        check_arg("confirm", 1, opts, "table")
        check_arg("confirm", 2, fn, "function")

        def answer(ok: bool):
            call_back(fn, ok, where="ctx.dialog:confirm callback")

        emu._set_pending("confirm", _opt_str(opts, "title"), _opt_str(opts, "text"), answer=answer)

    def toast(s=None, *_):
        check_arg("toast", 1, s, "string")
        emu._show_toast(s)

    ctx.input = make_object(L, "input", {"text": input_text, "edit": input_edit})
    ctx.dialog = make_object(L, "dialog", {"confirm": confirm, "toast": toast})

    # ---- images, QR, layers ------------------------------------------------------------------

    def decode(data=None, *_):
        body = _bytes_arg("decode", 1, data)
        try:
            with Image.open(BytesIO(body)) as src:
                fmt = src.format
                img = _flatten(src)
        except Exception:
            return None, "decode_failed"
        if fmt == "PNG":
            emu._warn("image:decode: PNG may not decode on the device (older firmware closes the app); use JPEG")
        return new_image(img)[0]

    def qr_encode(text=None, *_):
        try:
            code = segno.make_qr(_bytes_arg("encode", 1, text).decode("utf-8"), error="m", boost_error=False)
        except (UnicodeDecodeError, segno.DataOverflowError, ValueError):
            return None, "invalid_argument"
        rows = b"".join(bytes(0 if cell else 255 for cell in row) for row in code.matrix)
        size = len(code.matrix)
        return new_image(Image.frombytes("L", (size, size), rows))[0]

    def layer_create(w=None, h=None, *_):
        check_arg("create", 1, w, "number")
        check_arg("create", 2, h, "number")
        w, h = round(w), round(h)
        if not (1 <= w <= MAX_LAYER_SIDE and 1 <= h <= MAX_LAYER_SIDE):
            return None, "invalid_argument"
        sub = Canvas(L, emu.fonts, w, h)
        sub.image_source = resolve
        sub.on_warning = emu._warn
        holder: dict = {}

        def draw(fn=None, *_):
            check_arg("draw", 1, fn, "function")
            iid = holder["id"]
            if iid not in emu._images:
                raise ValueError("layer:draw: the layer was released")
            sub.begin_frame()
            ok, _res = emu._protected(fn, sub.g, where="layer:draw callback")
            if ok:
                sub.end_frame()
                emu._images[iid] = sub.render(emu._images[iid])

        obj, holder["id"] = new_image(Image.new("L", (w, h), 255), {"draw": draw}, "layer")
        return obj

    ctx.image = _binary_object(L, "image", {"decode": decode})
    ctx.qr = _binary_object(L, "qr", {"encode": qr_encode})
    ctx.layers = make_object(L, "layers", {"create": layer_create})

