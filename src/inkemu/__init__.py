import os

if os.environ.get("INKEMU_IMPL") == "rust":
    from .rust_emu import Emu
else:
    from .emu import Emu

__all__ = ["Emu"]
