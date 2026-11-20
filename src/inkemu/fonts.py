import os

from PIL import ImageFont

FONT_FILES = [
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
]
LINE_HEIGHTS = {"small": 20, "medium": 24, "large": 30}


class Fonts:
    def __init__(self):
        path = next((p for p in FONT_FILES if os.path.exists(p)), None)
        self.path = path
        self._fonts = {}
        for name, lh in LINE_HEIGHTS.items():
            if path:
                self._fonts[name] = ImageFont.truetype(path, int(lh * 0.85))
            else:
                self._fonts[name] = ImageFont.load_default(int(lh * 0.85))

    def _name(self, name: str) -> str:
        return name if name in self._fonts else "medium"

    def line_height(self, name: str) -> int:
        return LINE_HEIGHTS[self._name(name)]

    def measure(self, name: str, s: str) -> int:
        # Frames are 1-bit: mono hinting makes glyphs wider than the anti-aliased default advance.
        return round(self._fonts[self._name(name)].getlength(s, mode="1"))

    def pil(self, name: str) -> ImageFont.FreeTypeFont:
        return self._fonts[self._name(name)]
