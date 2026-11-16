// No browser or npm dependencies: node --test crates/remote/tests/karaoke_page.test.cjs
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');

function page() {
  const html = readFileSync(join(__dirname, '../src/karaoke.html'), 'utf8');
  const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
  const elements = new Map();
  const timers = new Map();
  const requests = [];
  let sequence = 0;
  const element = () => {
    const classes = new Set();
    return {
      style: { setProperty() {} },
      classList: {
        toggle(name, on) { if (on) classes.add(name); else classes.delete(name); },
        add(name) { classes.add(name); },
        remove(name) { classes.delete(name); },
        contains: name => classes.has(name),
      },
      getAttribute() { return null; },
    };
  };
  const body = element();
  const context = vm.createContext({
    AbortController,
    document: {
      documentElement: element(), body,
      createElement: () => ({ getContext: () => ({}) }),
      getElementById: id => {
        if (!elements.has(id)) elements.set(id, element());
        return elements.get(id);
      },
    },
    performance: { now: () => 0 },
    addEventListener() {},
    requestAnimationFrame() {},
    setTimeout(fn, delay) {
      const id = ++sequence;
      timers.set(id, { fn, delay });
      return id;
    },
    clearTimeout: id => timers.delete(id),
    fetch(url, { signal }) {
      return new Promise((resolve, reject) => {
        requests.push({ url, signal, resolve: value => resolve({ json: async () => value }), reject });
        signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
      });
    },
  });
  vm.runInContext(script, context);
  return {
    requests, timers, body,
    button: id => elements.get(id),
    retry() {
      const entry = [...timers.entries()].find(([, timer]) => timer.delay === 250);
      assert.ok(entry, 'a completed request must schedule the next update');
      timers.delete(entry[0]);
      return entry[1].fn();
    },
  };
}

const idle = { state: 0, id: null, color: '#b69cff', t: 0 };
const flush = () => new Promise(resolve => setImmediate(resolve));

test('polling waits for the previous update and skips unavailable lyrics', async () => {
  const p = page();
  assert.equal(p.requests.length, 1);
  assert.equal(p.requests[0].url, '/karaoke.json');
  assert.ok([...p.timers.values()].every(timer => timer.delay !== 250),
    'a slow request must not start overlapping updates');
  p.requests[0].resolve(idle);
  await flush();
  assert.equal(p.requests.length, 1, 'null lyric IDs must not fetch stale lines');
  const next = p.retry();
  assert.equal(p.requests.length, 2);
  p.requests[1].resolve(idle);
  await next;
});

test('a stalled request aborts and polling recovers', async () => {
  const p = page();
  const deadline = [...p.timers.values()].find(timer => timer.delay === 5000);
  assert.ok(deadline);
  deadline.fn();
  await flush();
  assert.ok(p.requests[0].signal.aborted);
  const next = p.retry();
  p.requests[1].resolve(idle);
  await next;
  assert.equal(p.requests.length, 2);
});

test('network failures still schedule a retry', async () => {
  const p = page();
  p.requests[0].reject(new Error('offline'));
  await flush();
  const next = p.retry();
  p.requests[1].resolve(idle);
  await next;
});

test('the page follows openDJ until a viewer picks, and keeps a view on', async () => {
  const p = page();
  p.requests[0].resolve({ ...idle, state: 1, cover_on: true, lyrics_on: false });
  await flush();
  assert.ok(p.body.classList.contains('cover-only'));
  assert.ok(!p.body.classList.contains('cover'));
  // Turning the cover off here brings the lyrics back on this page.
  p.button('cover-button').onclick();
  assert.ok(!p.body.classList.contains('cover-only'));
  assert.ok(!p.button('cover-button').classList.contains('on'));
  assert.ok(p.button('lyrics-button').classList.contains('on'));
  // Then lyrics off brings the cover in.
  p.button('lyrics-button').onclick();
  assert.ok(p.body.classList.contains('cover-only'));
  const next = p.retry();
  p.requests[1].resolve({ ...idle, state: 1, cover_on: false, lyrics_on: true });
  await next;
  assert.ok(p.body.classList.contains('cover-only'), "the viewer's own pick stays");
});
