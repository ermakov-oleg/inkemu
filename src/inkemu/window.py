import base64
import io
import signal
import threading
import time
import tkinter as tk
from datetime import datetime
from tkinter import messagebox, simpledialog

from .daemon import WindowUnavailable
from .emu import HOLD_MS, Emu

TICK_MS = 100
CONTROL_POLL_MS = 50
MAX_STEP_MS = 5000  # a suspended laptop must not fast-forward the app by hours

KEYMAP = {
    "up": "up",
    "down": "down",
    "left": "left",
    "right": "right",
    "return": "ok",
    "kp_enter": "ok",
    "escape": "back",
}


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
        self._closed = False
        self.interrupted = False
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
        self.root.protocol("WM_DELETE_WINDOW", self.close)
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
        return (
            min(max(int(event.x), 0), self.emu.canvas.width - 1),
            min(max(int(event.y), 0), self.emu.canvas.height - 1),
        )

    def refresh(self) -> None:
        try:
            img = self.emu.shot(dialog=False).convert("L")  # the Tk dialog shows it
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
        if not emu.running and not emu.locked:
            flags.append("stopped (R to restart)")
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
        if self._down and self._input_open():
            self._guard(self.emu.pointer, "move", *self._xy(event))

    def _fire_hold(self, x: int, y: int) -> None:
        self._hold_job = None
        if self._down and self._input_open():
            self._guard(self.emu.pointer, "hold", x, y)
            self.refresh()

    def on_release(self, event) -> None:
        if not self._down:
            return
        self._down = False
        if self._hold_job:
            self.root.after_cancel(self._hold_job)
            self._hold_job = None
        if self._input_open():
            self._advance()  # real time since the last tick must reach the clock before `up` classifies
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
        self._down = False  # the dialog's grab swallows the release
        if self._hold_job:
            self.root.after_cancel(self._hold_job)
            self._hold_job = None
        try:
            p, asked = dict(emu.pending), emu._pending_h
            if p["kind"] in ("text", "edit"):
                value = ask_string(self.root, p["title"], p["text"] if p["kind"] == "edit" else "", p["text"])
            else:
                value = ask_yes_no(self.root, p["title"], p["text"])
            if emu._pending_h is not asked:  # answered remotely meanwhile; the click must not answer a newer dialog
                emu._log_line(f"window: the {p['kind']} dialog was already answered, the click is dropped")
            elif p["kind"] not in ("text", "edit"):
                self._guard(emu.answer, value)
            elif value is None:
                self._guard(emu.answer, False)
            else:
                self._guard(emu.type_text, value)
        finally:
            self._dialog_open = False
            self._last_tick = time.monotonic()
        self.refresh()
        if emu.pending is not None:
            self.root.after_idle(self._open_dialog)

    # ---- time and remote control --------------------------------------------------------------

    def _advance(self) -> None:
        now = time.monotonic()
        elapsed = int((now - self._last_tick) * 1000)
        self._last_tick = now
        if elapsed > 0 and not self.emu.paused and not self._dialog_open:
            self._guard(self.emu.wait, min(elapsed, MAX_STEP_MS))

    def close(self) -> None:
        """Window closed: leave the app so on_leave runs and the state is saved."""
        self._closed = True
        self._guard(self.emu.leave)
        self.root.destroy()

    def _tick(self) -> None:
        if self.interrupted:
            self.close()
            return
        self._advance()
        self.refresh()
        self.root.after(TICK_MS, self._tick)

    def _poll_control(self) -> None:
        try:
            ran = self._guard(self.control.pump)
            if ran:
                self.on_remote_command()
                self.refresh()
            if getattr(self.control, "stopped", False):
                self.close()
                return
        finally:
            if not self._closed:
                self.root.after(CONTROL_POLL_MS, self._poll_control)

    def on_remote_command(self) -> None:
        """Remote commands freeze the clock (execute sets emu.paused); `resume` or P hands it back to real time."""
        self.remote_frozen = self.emu.paused
        self._last_tick = time.monotonic()


def build_window(emu: Emu, control=None, root: tk.Tk | None = None) -> WindowApp:
    return WindowApp(emu, control, root)


def run_window(emu: Emu, control=None) -> None:
    try:
        app = build_window(emu, control)
    except tk.TclError as e:
        raise WindowUnavailable(f"cannot open the window: {e}") from None
    # Ctrl-C would raise inside a Tk callback and only print a traceback; close from the next tick instead
    main = threading.current_thread() is threading.main_thread()
    old = signal.signal(signal.SIGINT, lambda *_: setattr(app, "interrupted", True)) if main else None
    try:
        app.root.mainloop()
    except KeyboardInterrupt:
        if not app._closed:
            app.close()
    finally:
        if main:
            signal.signal(signal.SIGINT, old)
