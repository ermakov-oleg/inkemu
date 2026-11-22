// Mouse/touch on the screen to core commands: click = tap, double click = double_tap, drag = swipe,
// long press = hold (window.py on_press/on_motion/_fire_hold/on_release, plus the spec's double tap).
export const SLOP_PX = 20; // the core's tap limit (input.rs classify)
export const HOLD_MS = 600;
export const DOUBLE_MS = 300;

const far = (ax, ay, bx, by) => Math.max(Math.abs(ax - bx), Math.abs(ay - by)) >= SLOP_PX;

/**
 * A press is held back until it is clearly not a tap (moved 20 px or held 600 ms: then it goes out live from its
 * `down`). A quick click waits up to 300 ms for a second one nearby: both together are one `double_tap`, otherwise
 * the click goes out as `down` + `up`. `send(cmd)` gets core commands.
 */
export class PointerInput {
  #send;
  #now;
  #setTimer;
  #clearTimer;
  #cur = null; // the press in progress: {x0, y0, t0, x, y, live}
  #first = null; // a quick click waiting for a second: {x0, y0, x, y, timer}
  #holdTimer = null;

  // wrapped: browsers throw "Illegal invocation" for setTimeout called with this = the instance
  constructor({ send, now = () => performance.now(), setTimer = (f, ms) => setTimeout(f, ms), clearTimer = (id) => clearTimeout(id) }) {
    this.#send = send;
    this.#now = now;
    this.#setTimer = setTimer;
    this.#clearTimer = clearTimer;
  }

  get pressed() {
    return this.#cur !== null;
  }

  down(x, y) {
    if (this.#cur) return;
    this.#cur = { x0: x, y0: y, t0: this.#now(), x, y, live: false };
    if (this.#first) {
      this.#clearTimer(this.#first.timer); // the second click came in time; its `up` decides
      if (far(this.#first.x0, this.#first.y0, x, y)) this.#flushFirst();
    }
    this.#holdTimer = this.#setTimer(() => {
      this.#holdTimer = null;
      if (!this.#cur) return;
      this.#goLive();
      this.#pointer('hold', this.#cur.x0, this.#cur.y0);
    }, HOLD_MS);
  }

  move(x, y) {
    const c = this.#cur;
    if (!c || (x === c.x && y === c.y)) return;
    c.x = x;
    c.y = y;
    if (!c.live && far(c.x0, c.y0, x, y)) this.#goLive();
    if (c.live) this.#pointer('move', x, y);
  }

  up(x, y) {
    const c = this.#cur;
    if (!c) return;
    this.#cur = null;
    if (this.#holdTimer) {
      this.#clearTimer(this.#holdTimer);
      this.#holdTimer = null;
    }
    const quick = !c.live && this.#now() - c.t0 < HOLD_MS && !far(c.x0, c.y0, x, y);
    if (!quick) {
      if (!c.live) {
        this.#flushFirst();
        this.#pointer('down', c.x0, c.y0);
      }
      this.#pointer('up', x, y);
      return;
    }
    if (this.#first) {
      const f = this.#first;
      this.#first = null;
      this.#send({ cmd: 'double_tap', x: f.x0, y: f.y0 });
      return;
    }
    const first = { x0: c.x0, y0: c.y0, x, y, timer: null };
    first.timer = this.#setTimer(() => {
      if (this.#first === first && !this.#cur) this.#flushFirst();
    }, DOUBLE_MS);
    this.#first = first;
  }

  /** A click still waiting for a second one goes out now, so a key pressed right after it lands after it. */
  flush() {
    if (this.#first && !this.#cur) this.#flushFirst();
  }

  #goLive() {
    const c = this.#cur;
    if (c.live) return;
    this.#flushFirst();
    c.live = true;
    this.#pointer('down', c.x0, c.y0);
  }

  /** The waiting click is a single tap after all. */
  #flushFirst() {
    const f = this.#first;
    if (!f) return;
    this.#first = null;
    this.#clearTimer(f.timer);
    this.#pointer('down', f.x0, f.y0);
    this.#pointer('up', f.x, f.y);
  }

  #pointer(phase, x, y) {
    this.#send({ cmd: 'pointer', phase, x, y });
  }
}
