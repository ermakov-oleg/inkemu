import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PointerInput } from '../js/pointer.js';

/** A PointerInput on a fake clock: `at(ms)` runs the timers due by then. */
function rig() {
  const sent = [];
  let t = 0;
  let timers = [];
  let nextId = 1;
  const p = new PointerInput({
    send: (c) => sent.push(c.cmd === 'pointer' ? `${c.phase} ${c.x},${c.y}` : `${c.cmd} ${c.x},${c.y}`),
    now: () => t,
    setTimer: (f, ms) => {
      const id = nextId++;
      timers.push({ id, due: t + ms, f });
      return id;
    },
    clearTimer: (id) => {
      timers = timers.filter((x) => x.id !== id);
    },
  });
  const at = (ms) => {
    for (;;) {
      const due = timers.filter((x) => x.due <= ms).sort((a, b) => a.due - b.due)[0];
      if (!due) break;
      timers = timers.filter((x) => x !== due);
      t = due.due;
      due.f();
    }
    t = ms;
  };
  return { p, sent, at };
}

test('a single click goes out as a tap once no second click came within 300 ms', () => {
  const { p, sent, at } = rig();
  p.down(100, 200);
  at(80);
  p.up(103, 201);
  assert.deepEqual(sent, []);
  at(379);
  assert.deepEqual(sent, []);
  at(400);
  assert.deepEqual(sent, ['down 100,200', 'up 103,201']);
});

test('two quick clicks close together are one double_tap at the first click', () => {
  const { p, sent, at } = rig();
  p.down(100, 200);
  at(80);
  p.up(100, 200);
  at(250);
  p.down(110, 205);
  at(330); // past the first click's 300 ms: the second press already came, its up decides
  p.up(110, 205);
  at(2000);
  assert.deepEqual(sent, ['double_tap 100,200']);
});

test('a second click far away: two taps', () => {
  const { p, sent, at } = rig();
  p.down(100, 200);
  at(50);
  p.up(100, 200);
  at(150);
  p.down(300, 200);
  assert.deepEqual(sent, ['down 100,200', 'up 100,200']);
  at(200);
  p.up(300, 200);
  at(1000);
  assert.deepEqual(sent, ['down 100,200', 'up 100,200', 'down 300,200', 'up 300,200']);
});

test('a drag goes out live from its down; a waiting click goes out first', () => {
  const { p, sent, at } = rig();
  p.down(50, 50);
  at(40);
  p.up(50, 50);
  at(100);
  p.down(52, 52);
  p.move(60, 52);
  assert.deepEqual(sent, []);
  p.move(80, 52);
  p.move(140, 52);
  at(200);
  p.up(140, 52);
  assert.deepEqual(sent, ['down 50,50', 'up 50,50', 'down 52,52', 'move 80,52', 'move 140,52', 'up 140,52']);
});

test('a long press sends down and hold at 600 ms, at the press point, then up', () => {
  const { p, sent, at } = rig();
  p.down(10, 20);
  at(599);
  assert.deepEqual(sent, []);
  at(600);
  assert.deepEqual(sent, ['down 10,20', 'hold 10,20']);
  p.move(12, 21);
  at(900);
  p.up(12, 21);
  assert.deepEqual(sent, ['down 10,20', 'hold 10,20', 'move 12,21', 'up 12,21']);
});

test('a quick click followed by a long press: the tap, then the hold', () => {
  const { p, sent, at } = rig();
  p.down(10, 20);
  at(50);
  p.up(10, 20);
  at(200);
  p.down(10, 20);
  at(800);
  p.up(10, 20);
  assert.deepEqual(sent, ['down 10,20', 'up 10,20', 'down 10,20', 'hold 10,20', 'up 10,20']);
});

test('a second press while one is down is ignored', () => {
  const { p, sent, at } = rig();
  p.down(10, 20);
  p.down(300, 300);
  at(700);
  p.up(10, 20);
  assert.deepEqual(sent, ['down 10,20', 'hold 10,20', 'up 10,20']);
});

test('flush sends a waiting click at once (a key right after a click lands after it)', () => {
  const { p, sent, at } = rig();
  p.down(10, 20);
  at(50);
  p.up(10, 20);
  p.flush();
  assert.deepEqual(sent, ['down 10,20', 'up 10,20']);
  at(1000);
  assert.equal(sent.length, 2, 'the timer does not send it again');
});

test('the default timers work when called as methods (browsers reject setTimeout with another this)', async () => {
  const sent = [];
  const p = new PointerInput({ send: (c) => sent.push(c.phase) });
  p.down(1, 1);
  p.up(1, 1);
  await new Promise((r) => setTimeout(r, 350));
  assert.deepEqual(sent, ['down', 'up']);
});
