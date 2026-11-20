import base64
import io
import queue
import time
import tkinter as tk
from datetime import datetime
from pathlib import Path
from tkinter import messagebox, simpledialog

from .emu import HOLD_MS, Emu

TICK_MS = 100
CONTROL_POLL_MS = 50
MAX_STEP_MS = 5000  # a suspended laptop must not fast-forward the app by hours

KEYMAP = {"up": "up", "down": "down", "left": "left", "right": "right", "return": "ok", "kp_enter": "ok",
          "escape": "back"}


def ask_string(parent, title: str, text: str, initial: str) -> str | None:
    return simpledialog.askstring(title or "Input", text, initialvalue=initial, parent=parent)


def ask_yes_no(parent, title: str, text: str) -> bool:
    return messagebox.askyesno(title or "Confirm", text, parent=parent)


class WindowApp:
    """Tk front end of one Emu. Built separately from the main loop so tests can drive it with update()."""

    def __init__(self, emu: Emu, control=None, root: tk.Tk | None = None):
        self.emu = emu
        self.control = control
        self.root = root or tk.Tk()
        self.root.title(f"inkemu - {emu.app_id}")
        self.root.resizable(False, False)
        self.problem = ""
        self.remote_frozen = False
        self._down = False
        self._hold_job = None
        self._dialog_open = False
        self._last_tick = time.monotonic()
        self._frame_png = b""
        self._photo: tk.PhotoImage | None = None
        self.screen = tk.Label(self.root, borderwidth=0, highlightthickness=0, background="white")
        self.screen.pack()
        self.status = tk.Label(self.root, anchor="w", justify="left", font=("Menlo", 11), wraplength=emu.canvas.width)
        self.status.pack(fill="x")
        self.screen.bind("<ButtonPress-1>", self.on_press)
        self.screen.bind("<B1-Motion>", self.on_motion)
        self.screen.bind("<ButtonRelease-1>", self.on_release)
        self.root.bind("<Key>", self.on_key)
        self.root.focus_force()
        emu.on_pending = lambda _p: self.root.after_idle(self._open_dialog)
        self.refresh()
        if emu.pending:
            self.root.after_idle(self._open_dialog)
        self.root.after(TICK_MS, self._tick)
        if control is not None:
            self.root.after(CONTROL_POLL_MS, self._poll_control)

    # ---- helpers ----------------------------------------------------------------------------

    def _guard(self, fn, *args):
        """Run an emulator call; nothing may escape into the Tk loop."""
        try:
            return fn(*args)
        except Exception as e:
            self.problem = f"{type(e).__name__}: {e}"
        return None

    def _input_open(self) -> bool:
        return self.emu.pending is None and not self._dialog_open

    def _xy(self, event) -> tuple[int, int]:
        return (min(max(int(event.x), 0), self.emu.canvas.width - 1),
                min(max(int(event.y), 0), self.emu.canvas.height - 1))

    def refresh(self) -> None:
        try:
            img = self.emu.shot().convert("L")
            buf = io.BytesIO()
            img.save(buf, "PNG")
            png = buf.getvalue()
            if png != self._frame_png:
                self._frame_png = png
                self._photo = tk.PhotoImage(data=base64.b64encode(png))
                self.screen.configure(image=self._photo)
        except Exception as e:
            self.problem = f"render: {type(e).__name__}: {e}"
        self.status.configure(text=self.status_text())

    def status_text(self) -> str:
        emu = self.emu
        flags = []
        if emu.paused:
            flags.append("paused (remote)" if self.remote_frozen else "paused")
        if emu.locked:
            flags.append("locked")
        if emu.pending:
            flags.append(f"dialog: {emu.pending['kind']}")
        parts = [f"{emu.clock.now():%Y-%m-%d %H:%M:%S}", f"net {emu.net_mode}", f"cmds {emu.frame_commands}", *flags]
        err = self.problem or (emu.errors[-1].splitlines()[0] if emu.errors else "")
        if err:
            parts.append("error: " + err)
        return " | ".join(parts)

    # ---- mouse ------------------------------------------------------------------------------

    def on_press(self, event) -> None:
        if not self._input_open():
            return
        self._down = True
        self._guard(self.emu.pointer, "down", *self._xy(event))
        self._hold_job = self.root.after(HOLD_MS, self._fire_hold, *self._xy(event))
        self.refresh()

    def on_motion(self, event) -> None:
        if self._down:
            self._guard(self.emu.pointer, "move", *self._xy(event))

    def _fire_hold(self, x: int, y: int) -> None:
        self._hold_job = None
        if self._down:
            self._guard(self.emu.pointer, "hold", x, y)
            self.refresh()

    def on_release(self, event) -> None:
        if not self._down:
            return
        self._down = False
        if self._hold_job:
            self.root.after_cancel(self._hold_job)
            self._hold_job = None
        self._guard(self.emu.pointer, "up", *self._xy(event))
        self.refresh()

    # ---- keyboard ---------------------------------------------------------------------------

    def on_key(self, event) -> None:
        emu, k = self.emu, str(event.keysym).lower()
        if k == "p":
            emu.paused = not emu.paused
            self.remote_frozen = False
        elif k == "s":
            self._guard(self._save_shot)
        elif not self._input_open():
            return
        elif k in KEYMAP:
            self._guard(emu.key, KEYMAP[k])
        elif k == "l":
            self._guard(emu.unlock if emu.locked else emu.lock)
        elif k == "r":
            self._guard(emu.reload)
        elif k == "bracketright":
            self._guard(emu.wait, "1m")
        elif k == "braceright":
            self._guard(emu.wait, "1h")
        self.refresh()

    def _save_shot(self) -> None:
        path = self.emu.state_dir / "shots" / f"{datetime.now():%Y%m%d-%H%M%S}.png"
        self.emu.save_shot(path)
        self.problem = ""
        self.status.configure(text=f"saved {path}")

    # ---- dialogs ----------------------------------------------------------------------------

    def _open_dialog(self) -> None:
        emu = self.emu
        if self._dialog_open or emu.pending is None:
            return
        self._dialog_open = True
        try:
            p = dict(emu.pending)
            if p["kind"] in ("text", "edit"):
                value = ask_string(self.root, p["title"], p["text"] if p["kind"] == "edit" else "", p["text"])
                if value is None:
                    self._guard(emu.answer, False)
                else:
                    self._guard(emu.type_text, value)
            else:
                self._guard(emu.answer, ask_yes_no(self.root, p["title"], p["text"]))
        finally:
            self._dialog_open = False
            self._last_tick = time.monotonic()
        self.refresh()
        if emu.pending is not None:
            self.root.after_idle(self._open_dialog)

    # ---- time and remote control --------------------------------------------------------------

    def _tick(self) -> None:
        now = time.monotonic()
        elapsed = int((now - self._last_tick) * 1000)
        self._last_tick = now
        if elapsed > 0 and not self.emu.paused and not self._dialog_open:
            self._guard(self.emu.wait, min(elapsed, MAX_STEP_MS))
        self.refresh()
        self.root.after(TICK_MS, self._tick)

    def _poll_control(self) -> None:
        """Items of control.queue are zero-argument callables; Task 9's Daemon decides what they wrap."""
        try:
            while True:
                try:
                    item = self.control.queue.get_nowait()
                except queue.Empty:
                    break
                self.on_remote_command()
                self._guard(item)
                self.refresh()
        finally:
            self.root.after(CONTROL_POLL_MS, self._poll_control)

    def on_remote_command(self) -> None:
        """The first remote command freezes the window clock; P or `resume` hands it back to real time."""
        if not self.remote_frozen:
            self.remote_frozen = True
            self.emu.paused = True
        self._last_tick = time.monotonic()


def build_window(emu: Emu, control=None, root: tk.Tk | None = None) -> WindowApp:
    return WindowApp(emu, control, root)


def run_window(emu: Emu, control=None) -> None:
    app = build_window(emu, control)
    app.root.mainloop()
