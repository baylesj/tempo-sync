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
// Host clock reads 9 s when the page's performance.now() reads 0.
const OFFSET_US = 9_000_000;

function setup({ grant = true } = {}) {
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
        deliver({ type: 'state', state: STATE, numPeers: 1 });
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
    performance: { now: () => now },
  });
  window.window = window;
  vm.runInContext(source, context);
  return {
    tempoSync: vm.runInContext('navigator.tempoSync', context),
    sent,
    deliver,
    AudioContext,
    setNow: (ms) => (now = ms),
    flush: () => new Promise((r) => setTimeout(r, 0)),
  };
}

test('requestSession resolves once the bridge has state and a clock', async () => {
  const { tempoSync } = setup();
  const session = await tempoSync.requestSession({ quantum: 4 });
  assert.equal(session.numPeers, 1);
  assert.equal(session.quantum, 4);
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
  // Host 10 s is page 1 s; at 120 BPM, page 3 s is beat 4.
  assert.equal(state.timeAtBeat(0), 1);
  assert.equal(state.beatAtTime(3), 4);
  assert.equal(state.phaseAtTime(3.5), 1);
  assert.equal(state.phaseAtTime(0.5), 3);
});

test('captureState(audioContext) maps into the context time base', async () => {
  const { tempoSync, AudioContext, setNow } = setup();
  const session = await tempoSync.requestSession();
  setNow(2000);
  // This context's clock runs 0.25 s behind the page's.
  const state = session.captureState(new AudioContext(0.25));
  assert.equal(state.timeAtBeat(0), 0.75);
  assert.equal(session.captureState(undefined).timeAtBeat(0), 1);
  assert.throws(() => session.captureState(null), { name: 'TypeError' });
});

test('an unchanged state commits back to exactly the host state', async () => {
  const { tempoSync, sent } = setup();
  const session = await tempoSync.requestSession();
  session.commitState(session.captureState());
  assert.deepEqual({ ...sent.at(-1).state }, STATE);
});

test('setTempo keeps the beat at the change point and commits in host units', async () => {
  const { tempoSync, sent } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  state.setTempo(90, 3);
  assert.equal(state.beatAtTime(3), 4);
  assert.equal(state.beatAtTime(5), 7);
  session.commitState(state);
  const committed = sent.at(-1).state;
  assert.equal(committed.tempo, Math.round(60e6 / 90));
  assert.equal(committed.timeOrigin, 12_000_000);
  assert.equal(committed.beatOrigin, 4_000_000);
});

test('requestBeatAtTime moves forward to the next matching phase', async () => {
  const { tempoSync } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  // Page 3.5 s is beat 5 (phase 1 in 4); asking for beat 0 lands on beat 8.
  state.requestBeatAtTime(0, 3.5);
  assert.equal(state.beatAtTime(3.5), 8);
  assert.equal(state.phaseAtTime(3.5), 0);
});

test('setIsPlaying records the beat at the start time on commit', async () => {
  const { tempoSync, sent } = setup();
  const session = await tempoSync.requestSession();
  const state = session.captureState();
  state.setIsPlaying(true, 2);
  session.commitState(state);
  const committed = sent.at(-1).state;
  assert.equal(committed.isPlaying, true);
  assert.equal(committed.startStopTime, 11_000_000);
  assert.equal(committed.startStopBeat, 2_000_000);
});

test('commitState fires events locally and refuses misuse', async () => {
  const { tempoSync } = setup();
  const a = await tempoSync.requestSession();
  const b = await tempoSync.requestSession();
  const fired = [];
  a.addEventListener('tempochange', () => fired.push('a tempo'));
  b.ontempochange = () => fired.push('b tempo');
  b.onplayingchange = () => fired.push('b playing');

  const state = a.captureState();
  state.setTempo(100, 1);
  state.setIsPlaying(true, 1);
  a.commitState(state);
  assert.deepEqual(fired, ['a tempo', 'b tempo', 'b playing']);

  assert.throws(() => a.commitState(state), { name: 'InvalidStateError' });
  assert.throws(() => b.commitState(a.captureState()), { name: 'InvalidModificationError' });
  a.close();
  assert.throws(() => a.captureState(), { name: 'InvalidStateError' });
});

test('state from the host fires numpeerschange', async () => {
  const { tempoSync, deliver, flush } = setup();
  const session = await tempoSync.requestSession();
  let peers = null;
  session.onnumpeerschange = () => (peers = session.numPeers);
  deliver({ type: 'state', state: STATE, numPeers: 3 });
  await flush();
  assert.equal(peers, 3);
});

test('the interfaces cannot be constructed by pages', () => {
  const { tempoSync } = setup();
  assert.throws(() => new tempoSync.constructor(), { name: 'TypeError' });
});
