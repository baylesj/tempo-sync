// Runs page.js against a stand-in window that answers like bridge.js would.
// node --test extension/test/*.test.mjs

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../page.js', import.meta.url), 'utf8');
const TAG = '__webTempoSync';

// 120 BPM, beat 0 at host time 10 s, stopped since then.
const STATE = {
  tempo: 500000,
  beatOrigin: 0,
  timeOrigin: 10_000_000,
  isPlaying: false,
  startStopBeat: 0,
  startStopTime: 10_000_000,
};
// Host clock reads 9 s when the page's performance.now() reads 0, so host 10 s is page 1 s
// and page time t is shared beat 2 * (t - 1).
const OFFSET_US = 9_000_000;

function setup({ grant = true, numPeers = 1 } = {}) {
  const listeners = [];
  const sent = [];
  let now = 0;
  const window = {
    isSecureContext: true,
    addEventListener: (type, fn) => listeners.push(fn),
    postMessage(data) {
      if (data[TAG] !== 'page') {
        return;
      }
      sent.push(data.msg);
      if (data.msg.type === 'open') {
        deliver({ type: 'state', state: STATE, numPeers });
        deliver({ type: 'sessions', count: sent.filter((m) => m.type === 'open').length });
        deliver({ type: 'clock', offsetUs: OFFSET_US });
        deliver(grant
          ? { type: 'opened', id: data.msg.id }
          : { type: 'opened', id: data.msg.id, error: { name: 'NotAllowedError', message: 'denied' } });
      }
    },
  };
  function deliver(msg) {
    queueMicrotask(() => listeners.forEach((fn) => fn({ source: window, data: { [TAG]: 'ext', msg } })));
  }
  class Navigator {}
  class BaseAudioContext {}
  class AudioContext extends BaseAudioContext {
    constructor(offset) {
      super();
      this.offset = offset;
    }
    getOutputTimestamp() {
      return { performanceTime: now, contextTime: now / 1000 - this.offset };
    }
  }
  const context = vm.createContext({
    window,
    Navigator,
    navigator: new Navigator(),
    BaseAudioContext,
    AudioContext,
    EventTarget,
    Event,
    DOMException,
    console,
    queueMicrotask,
    setTimeout,
    performance: { now: () => now },
  });
  window.window = window;
  vm.runInContext(source, context);
  return {
    tempoSync: vm.runInContext('navigator.tempoSync', context),
    commits: () => sent.filter((m) => m.type === 'commit').map((m) => ({ ...m.state })),
    deliver,
    AudioContext,
    setNow: (ms) => (now = ms),
    flush: () => new Promise((r) => setTimeout(r, 5)),
  };
}

test('requestSession resolves once the bridge has state and a clock', async () => {
  const { tempoSync } = setup();
  const session = await tempoSync.requestSession({ quantum: 4 });
  assert.equal(session.hasPeers, true);
  assert.equal(session.quantum, 4);
});

test('hasPeers counts network peers and other sessions in the browser', async () => {
  const { tempoSync, deliver, flush } = setup({ numPeers: 0 });
  const session = await tempoSync.requestSession();
  assert.equal(session.hasPeers, false);
  let fired = 0;
  session.onpeerschange = () => fired++;
  deliver({ type: 'sessions', count: 2 });
  await flush();
  assert.equal(session.hasPeers, true);
  assert.equal(fired, 1);
});

test('requestSession validates quantum like the IDL and the draft', async () => {
  const { tempoSync } = setup();
  await assert.rejects(tempoSync.requestSession({ quantum: 0 }), { name: 'RangeError' });
  await assert.rejects(tempoSync.requestSession({ quantum: NaN }), { name: 'TypeError' });
});

test('a refused prompt rejects with NotAllowedError', async () => {
  const { tempoSync } = setup({ grant: false });
  await assert.rejects(tempoSync.requestSession(), { name: 'NotAllowedError' });
});

test('captureState maps host time into page seconds', async () => {
  const { tempoSync } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  assert.equal(state.tempo, 120);
  assert.equal(state.timeAtBeat(0), 1);
  assert.equal(state.beatAtTime(3), 4);
  assert.equal(state.phaseAtTime(3.5), 1);
  assert.equal(state.phaseAtTime(0.5), 3);
});

test('captureState(audioContext) maps into the context time base; null is a TypeError', async () => {
  const { tempoSync, AudioContext, setNow } = setup();
  const session = await tempoSync.requestSession();
  setNow(2000);
  // This context's clock runs 0.25 s behind the page's.
  assert.equal(session.captureState(new AudioContext(0.25)).timeAtBeat(0), 0.75);
  assert.equal(session.captureState(undefined).timeAtBeat(0), 1);
  assert.throws(() => session.captureState(null), { name: 'TypeError' });
});

test('committing an unchanged state shares nothing', async () => {
  const { tempoSync, commits } = setup();
  const session = await tempoSync.requestSession();
  session.commitState(session.captureState());
  assert.equal(commits().length, 0);
});

test('setTempo keeps the beat at the change point and commits in host units', async () => {
  const { tempoSync, commits } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  state.setTempo(90, 3);
  assert.equal(state.beatAtTime(3), 4);
  assert.equal(state.beatAtTime(5), 7);
  session.commitState(state);
  const [committed] = commits();
  assert.equal(committed.tempo, Math.round(60e6 / 90));
  assert.equal(committed.timeOrigin, 12_000_000);
  assert.equal(committed.beatOrigin, 4_000_000);
});

test('with others, requestBeatAtTime waits for the bar line and moves only our numbering', async () => {
  const { tempoSync, commits } = setup({ numPeers: 1 });
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  // Page 3.5 s is shared beat 5, phase 1. Beat 0 lands on the next bar: shared beat 8, at 5 s.
  state.requestBeatAtTime(0, 3.5);
  assert.equal(state.timeAtBeat(0), 5);
  assert.equal(state.beatAtTime(5), 0);
  assert.equal(state.beatAtTime(3.5), -3);
  assert.equal(state.phaseAtTime(3.5), 1, 'the shared phase is unchanged');
  session.commitState(state);
  assert.equal(commits().length, 0, 'nothing is shared');
  assert.equal(session.captureState().beatAtTime(5), 0, 'the session keeps its numbering');
});

test('alone, requestBeatAtTime puts the beat exactly at the time', async () => {
  const { tempoSync, commits } = setup({ numPeers: 0 });
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  state.requestBeatAtTime(0, 3.5);
  assert.equal(state.beatAtTime(3.5), 0);
  assert.equal(state.phaseAtTime(3.5), 0);
  session.commitState(state);
  assert.equal(commits().length, 1);
});

test('forceBeatAtTime moves the shared bar lines by at most half a quantum', async () => {
  const { tempoSync, commits } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  // Shared beat 5 (phase 1) at 3.5 s; forcing beat 0 shifts back one beat, not forward three.
  state.forceBeatAtTime(0, 3.5);
  assert.equal(state.beatAtTime(3.5), 0);
  assert.equal(state.phaseAtTime(3.5), 0);
  session.commitState(state);
  assert.equal(commits()[0].beatOrigin, 4_000_000);
  assert.equal(commits()[0].timeOrigin, 12_500_000);
});

test('beats keep the shared phase for every quantum', async () => {
  const { tempoSync } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  state.requestBeatAtTime(0, 3.5, 4);
  for (const q of [3, 4, 7]) {
    const local = state.beatAtTime(4.25, q);
    assert.ok(Math.abs((((local % q) + q) % q) - state.phaseAtTime(4.25, q)) < 1e-9, `quantum ${q}`);
    assert.ok(Math.abs(state.timeAtBeat(local, q) - 4.25) < 1e-9, `round trip for quantum ${q}`);
  }
});

test("a stale snapshot doesn't undo another participant's tempo change", async () => {
  const { tempoSync, deliver, flush, commits } = setup();
  const session = await tempoSync.requestSession();
  const stale = session.captureState();
  deliver({ type: 'state', state: { ...STATE, tempo: 600000 }, numPeers: 1 });
  await flush();
  stale.setIsPlaying(true, 2);
  session.commitState(stale);
  const [committed] = commits();
  assert.equal(committed.tempo, 600000);
  assert.equal(committed.isPlaying, true);
  assert.equal(committed.startStopTime, 11_000_000);
});

test('events are queued, and fire for real changes only', async () => {
  const { tempoSync, deliver, flush } = setup();
  const a = await tempoSync.requestSession();
  const b = await tempoSync.requestSession();
  await flush();
  const fired = [];
  a.addEventListener('timelinechange', () => fired.push('a timeline'));
  b.ontimelinechange = () => fired.push('b timeline');
  b.onplayingchange = () => fired.push('b playing');

  const state = a.captureState();
  state.setTempo(100, 1);
  state.setIsPlaying(true, 1);
  a.commitState(state);
  assert.deepEqual(fired, [], 'not during commitState');
  await flush();
  assert.deepEqual(fired, ['a timeline', 'b timeline', 'b playing']);

  // The host echoes the commit re-anchored to a later origin: the same line, so no event.
  fired.length = 0;
  deliver({ type: 'state', state: { ...STATE, tempo: 600000, timeOrigin: 16_000_000, beatOrigin: 10_000_000, isPlaying: true, startStopTime: 10_000_000 }, numPeers: 1 });
  await flush();
  assert.deepEqual(fired, []);
});

test('misuse throws the draft’s exceptions', async () => {
  const { tempoSync } = setup();
  const a = await tempoSync.requestSession();
  const b = await tempoSync.requestSession();
  const state = a.captureState();
  state.setTempo(100, 1);
  a.commitState(state);
  assert.throws(() => a.commitState(state), { name: 'InvalidStateError' });
  assert.throws(() => b.commitState(a.captureState()), { name: 'InvalidModificationError' });
  a.close();
  assert.throws(() => a.captureState(), { name: 'InvalidStateError' });
});

test('the interfaces cannot be constructed by pages', () => {
  const { tempoSync } = setup();
  assert.throws(() => new tempoSync.constructor(), { name: 'TypeError' });
});
