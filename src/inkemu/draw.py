import unicodedata
from collections.abc import Callable
from typing import Any

from PIL import Image, ImageDraw

from .fonts import Fonts
from .lua_env import make_object

CAP = 1024
_RECORDED = (
    "clear",
    "rect",
    "round_rect",
    "line",
    "circle",
    "arc",
    "polygon",
    "polyline",
    "text",
    "text_box",
    "image",
    "pattern_rect",
    "fill_pattern",
)
_RECORDER = """
local rec, cap, specs = ...
local KINDS = { n = 'number', t = 'table', s = 'string' }
local function check(name, spec, ...)
  local args = table.pack(...)
  for i = 1, #spec do
    local k, v = spec:sub(i, i), args[i]
    local ok = k == 'a' and v ~= nil or type(v) == KINDS[k] or (k == 's' and type(v) == 'number')
    if not ok then
      error("bad argument #" .. i .. " to '" .. name .. "' (" .. (KINDS[k] or 'value') .. " expected, got "
        .. (v == nil and 'no value' or type(v)) .. ")", 3)
    end
  end
end
return function(name)
  local spec = specs[name]
  return function(...)
    check(name, spec, ...)
    rec.n = rec.n + 1
    if rec.n <= cap then rec.cmds[rec.n] = table.pack(name, ...) end
  end
end
"""


def _is_table(v) -> bool:
    return hasattr(v, "items") and hasattr(v, "keys")


def _conv(v):
    """Lua table -> list (sequence) or dict, recursively; other values untouched."""
    if not _is_table(v):
        return v
    d = {k: _conv(x) for k, x in v.items()}
    n = len(d)
    if n and all(isinstance(k, int) for k in d) and set(d) == set(range(1, n + 1)):
        return [d[i] for i in range(1, n + 1)]
    return d


def _font_name(f) -> str:
    if isinstance(f, dict):
        f = f.get("name")
    return f if isinstance(f, str) else "medium"


def _has_rtl(s: str) -> bool:
    return any(unicodedata.bidirectional(ch) in ("R", "AL") for ch in s)


def _points(pts) -> list[float]:
    if not isinstance(pts, list):
        return []
    flat: list[float] = []
    for p in pts:
        if isinstance(p, list):
            flat.extend(p[:2])
        elif isinstance(p, dict):
            flat.extend((p.get("x", 0), p.get("y", 0)))
        else:
            flat.append(p)
    return flat


_SPECS = {
    "clear": "",
    "rect": "nnnn",
    "round_rect": "nnnn",
    "line": "nnnn",
    "circle": "nnn",
    "arc": "nnnnn",
    "polygon": "t",
    "polyline": "t",
    "text": "nns",
    "text_box": "nnnns",
    "image": "ann",
    "pattern_rect": "nnnn",
    "fill_pattern": "nnnn",
}


class Canvas:
    def __init__(self, L, fonts: Fonts, width: int = 480, height: int = 800):
        self.L, self.fonts, self.width, self.height = L, fonts, width, height
        self.on_warning: Callable[[str], None] = lambda s: None
        self.image_source: Callable[[Any], Image.Image | None] = lambda obj: None
        self._cmds: list[tuple] = []
        self._texts: list[dict] = []
        self._checker_cache: dict[tuple[int, int], Image.Image] = {}
        self._rec = L.table_from({"n": 0, "cmds": L.table()})
        make_recorder = L.execute(_RECORDER, self._rec, CAP, L.table_from(_SPECS))
        methods = {name: make_recorder(name) for name in _RECORDED}
        methods.update(size=lambda: (self.width, self.height), measure=self._measure, measure_text=self._measure)
        self.g = make_object(L, "g", methods)

    def _measure(self, s, opts=None) -> int:
        o = _conv(opts) if opts is not None else {}
        return self.fonts.measure(_font_name(o.get("font") if isinstance(o, dict) else None), str(s))

    def resize(self, w: int, h: int) -> None:
        self.width, self.height = w, h

    def begin_frame(self) -> None:
        self._rec["n"] = 0
        self._rec["cmds"] = self.L.table()
        self._cmds, self._texts = [], []

    def end_frame(self) -> int:
        n = self._rec["n"]
        if n > CAP:
            self.on_warning(f"{n} draw commands in one frame; the device drops everything after {CAP}")
        cmds, texts = [], []
        for i in range(1, min(n, CAP) + 1):
            entry = self._rec["cmds"][i]
            count = entry["n"]
            raw = [entry[j] for j in range(1, count + 1)]
            op, rest = raw[0], raw[1:]
            args = [_conv(a) for a in rest]
            if op == "image" and rest:
                args[0] = self.image_source(rest[0])
            layout = None
            if op in ("text", "text_box"):
                layout = self._layout(op, args)
                if layout:
                    texts.append(layout["item"])
            cmds.append((op, args, layout))
        self._cmds, self._texts = cmds, texts
        return n

    def texts(self) -> list[dict]:
        return list(self._texts)

    def _layout(self, op: str, args: list) -> dict | None:
        if op == "text":
            x, y, s = args[0], args[1], str(args[2])
            opts = args[3] if len(args) > 3 and isinstance(args[3], dict) else {}
        else:
            x, y, w, h, s = args[0], args[1], args[2], args[3], str(args[4])
            opts = args[5] if len(args) > 5 and isinstance(args[5], dict) else {}
        if _has_rtl(s):
            self.on_warning(f"g:{op} with RTL text is not drawn by the device: {s!r}")
            return None
        font = _font_name(opts.get("font"))
        lh = self.fonts.line_height(font)
        align, valign = opts.get("align", "left"), opts.get("valign", "top")
        if op == "text":
            bw, bh = self.fonts.measure(font, s), lh
            bx = round(x - bw / 2) if align == "center" else round(x - bw) if align == "right" else round(x)
            by = round(y - bh / 2) if valign == "middle" else round(y - bh) if valign == "bottom" else round(y)
            line_boxes = [(bx, by, s)]
            item = {"text": s, "x": bx, "y": by, "w": bw, "h": bh, "font": font}
        else:
            lines = self._wrap(font, s, w)[: max(1, int(h // lh))]
            bh = len(lines) * lh
            by = (
                round(y + (h - bh) / 2) if valign == "middle" else round(y + h - bh) if valign == "bottom" else round(y)
            )
            line_boxes = []
            for i, line in enumerate(lines):
                lw = self.fonts.measure(font, line)
                lx = x + (w - lw) / 2 if align == "center" else x + w - lw if align == "right" else x
                line_boxes.append((round(lx), by + i * lh, line))
            item = {"text": s, "x": round(x), "y": by, "w": round(w), "h": bh, "font": font}
        return {"item": item, "lines": line_boxes, "font": font}

    def _wrap(self, font: str, s: str, width: float) -> list[str]:
        out: list[str] = []
        for para in s.split("\n"):
            cur = ""
            for word in para.split(" "):
                cand = word if not cur else cur + " " + word
                if self.fonts.measure(font, cand) <= width:
                    cur = cand
                    continue
                if cur:
                    out.append(cur)
                cur = ""
                while self.fonts.measure(font, word) > width and len(word) > 1:
                    k = len(word) - 1
                    while k > 1 and self.fonts.measure(font, word[:k]) > width:
                        k -= 1
                    out.append(word[:k])
                    word = word[k:]
                cur = word
            out.append(cur)
        return out

    def _checker(self, box: tuple[int, int, int, int]) -> Image.Image:
        size = (self.width, self.height)
        mask = self._checker_cache.get(size)
        if mask is None:
            stride = (self.width + 7) // 8
            data = b"".join((b"\xaa" if y % 2 == 0 else b"\x55") * stride for y in range(self.height))
            mask = self._checker_cache[size] = Image.frombytes("1", size, data)
        return mask.crop(box)

    def render(self, base: Image.Image | None = None) -> Image.Image:
        img = base.convert("1") if base is not None else Image.new("1", (self.width, self.height), 255)
        d = ImageDraw.Draw(img)
        for op, args, layout in self._cmds:
            self._draw(img, d, op, args, layout)
        return img

    def _draw(self, img: Image.Image, d: ImageDraw.ImageDraw, op: str, a: list, layout: dict | None) -> None:
        def opts_at(i: int) -> dict:
            return a[i] if len(a) > i and isinstance(a[i], dict) else {}

        def ink(o: dict) -> int:
            return 255 if o.get("color") == 0 else 0

        def lw(o: dict) -> int:
            return max(1, int(o.get("line_width", o.get("width", 1))))

        if op == "clear":
            c = a[0] if a and not isinstance(a[0], dict) else 0
            d.rectangle([0, 0, self.width, self.height], fill=255 if c == 0 else 0)
        elif op in ("rect", "round_rect"):
            x, y, w, h = a[:4]
            if w <= 0 or h <= 0:
                return
            has_r = op == "round_rect" and len(a) > 4 and not isinstance(a[4], dict)
            o = opts_at(5 if has_r else 4)
            radius = a[4] if has_r else o.get("radius", 8 if op == "round_rect" else 0)
            box = [x, y, x + w - 1, y + h - 1]
            fill = ink(o) if o.get("fill") else None
            if radius:
                d.rounded_rectangle(box, radius=radius, outline=ink(o), fill=fill, width=lw(o))
            else:
                d.rectangle(box, outline=ink(o), fill=fill, width=lw(o))
        elif op == "line":
            o = opts_at(4)
            d.line(a[:4], fill=ink(o), width=lw(o))
        elif op == "circle":
            x, y, r = a[:3]
            o = opts_at(3)
            d.ellipse([x - r, y - r, x + r, y + r], outline=ink(o), fill=ink(o) if o.get("fill") else None, width=lw(o))
        elif op == "arc":
            x, y, r, a0, a1 = a[:5]
            o = opts_at(5)
            if o.get("fill"):
                d.pieslice([x - r, y - r, x + r, y + r], a0, a1, fill=ink(o))
            else:
                d.arc([x - r, y - r, x + r, y + r], a0, a1, fill=ink(o), width=lw(o))
        elif op == "polygon":
            o, pts = opts_at(1), _points(a[0] if a else None)
            if len(pts) >= 6:
                d.polygon(pts, outline=ink(o), fill=ink(o) if o.get("fill") else None, width=lw(o))
        elif op == "polyline":
            o, pts = opts_at(1), _points(a[0] if a else None)
            if len(pts) >= 4:
                d.line(pts, fill=ink(o), width=lw(o), joint="curve")
        elif op in ("text", "text_box"):
            if layout:
                f = self.fonts.pil(layout["font"])
                lh = self.fonts.line_height(layout["font"])
                for lx, ly, line in layout["lines"]:
                    d.text((lx, ly + (lh - f.size) / 2), line, font=f, fill=0)
        elif op in ("pattern_rect", "fill_pattern"):
            x, y, w, h = (int(v) for v in a[:4])
            x0, y0, x1, y1 = max(x, 0), max(y, 0), min(x + w, self.width), min(y + h, self.height)
            if x1 > x0 and y1 > y0:
                img.paste(0, (x0, y0), self._checker((x0, y0, x1, y1)))
        elif op == "image":
            src = a[0]
            if src is None:
                return
            o = opts_at(3)
            ow, oh = int(o.get("w", src.width)), int(o.get("h", src.height))
            src = src.convert("L")
            if (ow, oh) != src.size:
                src = src.resize((ow, oh), Image.NEAREST if ow >= src.width else Image.LANCZOS)
            img.paste(src.convert("1"), (int(a[1]), int(a[2])))
