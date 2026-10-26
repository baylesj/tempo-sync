// navigator.tempoSync, following the Web Tempo Sync draft (https://bayl.es/web-tempo-sync/).
//
// Runs in the page's own world so pages get ordinary objects. bridge.js, in the extension's
// isolated world, carries messages to the service worker and keeps the clock offset.
//
// The host keeps session times as integers in its own clock: microseconds, micro-beats and
// microseconds per beat. They're converted to and from page seconds only at captureState()
// and commitState(), so a state captured and committed unchanged round-trips exactly.
(() => {
  if (!window.isSecureContext || 'tempoSync' in Navigator.prototype) {
    return;
  }

  const TAG = '__webTempoSync';
  const token = Symbol('internal');
  const illegal = () => new TypeError('Illegal constructor');

  let hostState = null;
  let numPeers = 0;
  // host µs = performance.now() * 1000 + offsetUs, measured by bridge.js.
  let offsetUs = 0;
  const sessions = new Set();
  const opening = new Map();
  let nextId = 1;

  // Set inside the classes' static blocks, so module code can reach their private fields.
  let notifySession;
  let readState;

  const post = (msg) => window.postMessage({ [TAG]: 'page', msg }, '*');

  // WebIDL `double`: anything that converts to a non-finite number is a TypeError.
  function toDouble(value, name) {
    const n = Number(value);
    if (!Number.isFinite(n)) {
      throw new TypeError(`${name} is not a finite number`);
    }
    return n;
  }

  // AudioContext time = page seconds - audioOffset(context). getOutputTimestamp() pairs the
  // context time now reaching the speakers with the page time it got there, so this maps to
  // hardware time. It steps by a render quantum, so follow it slowly unless it jumps
  // (device change, suspend and resume).
  const audioOffsets = new WeakMap();
  function audioOffset(context) {
    if (!(context instanceof AudioContext)) {
      return 0;
    }
    const ts = context.getOutputTimestamp();
    const sample = ts.performanceTime > 0
      ? ts.performanceTime / 1000 - ts.contextTime
      : performance.now() / 1000 - context.currentTime;
    const prev = audioOffsets.get(context);
    const next = prev === undefined || Math.abs(sample - prev) > 0.02
      ? sample
      : prev + (sample - prev) * 0.05;
    audioOffsets.set(context, next);
    return next;
  }

  function applyState(state, peers) {
    const prev = hostState;
    const prevPeers = numPeers;
    hostState = state;
    numPeers = peers;
    for (const session of sessions) {
      notifySession(session, prev, prevPeers);
    }
  }

  function defineHandler(proto, type) {
    const key = Symbol(type);
    Object.defineProperty(proto, `on${type}`, {
      configurable: true,
      enumerable: true,
      get() {
        return this[key] ?? null;
      },
      set(fn) {
        if (this[key]) {
          this.removeEventListener(type, this[key]);
        }
        this[key] = typeof fn === 'function' ? fn : null;
        if (this[key]) {
          this.addEventListener(type, this[key]);
        }
      },
    });
  }

  class TempoSessionState {
    #session;
    #toHost;
    #tempo;
    #beatOrigin;
    #timeOrigin;
    #quantum;
    #isPlaying;
    #timeForIsPlaying;
    // Micro-beats as the host had them, until setIsPlaying() makes us work it out again.
    #startStopBeat;
    #committed = false;

    constructor(t, session, fields, toHost) {
      if (t !== token) {
        throw illegal();
      }
      this.#session = session;
      this.#toHost = toHost;
      this.#tempo = fields.tempo;
      this.#beatOrigin = fields.beatOrigin;
      this.#timeOrigin = fields.timeOrigin;
      this.#quantum = fields.quantum;
      this.#isPlaying = fields.isPlaying;
      this.#timeForIsPlaying = fields.timeForIsPlaying;
      this.#startStopBeat = fields.startStopBeat;
    }

    #q(quantum) {
      if (quantum === undefined) {
        return this.#quantum;
      }
      const q = toDouble(quantum, 'quantum');
      return q > 0 ? q : this.#quantum;
    }

    get tempo() {
      return this.#tempo;
    }

    get isPlaying() {
      return this.#isPlaying;
    }

    get timeForIsPlaying() {
      return this.#timeForIsPlaying;
    }

    setTempo(bpm, atTime) {
      bpm = toDouble(bpm, 'bpm');
      atTime = toDouble(atTime, 'atTime');
      if (bpm <= 0) {
        throw new RangeError('bpm must be positive');
      }
      const currentBeat = this.beatAtTime(atTime);
      this.#timeOrigin = atTime;
      this.#beatOrigin = currentBeat;
      this.#tempo = bpm;
    }

    // `quantum` is accepted and ignored here, as in the draft (see its issue on this method).
    beatAtTime(time, quantum) {
      time = toDouble(time, 'time');
      this.#q(quantum);
      return this.#beatOrigin + (time - this.#timeOrigin) * (this.#tempo / 60);
    }

    phaseAtTime(time, quantum) {
      const q = this.#q(quantum);
      const beat = this.beatAtTime(time);
      return ((beat % q) + q) % q;
    }

    timeAtBeat(beat, quantum) {
      beat = toDouble(beat, 'beat');
      this.#q(quantum);
      return this.#timeOrigin + (beat - this.#beatOrigin) * (60 / this.#tempo);
    }

    requestBeatAtTime(beat, time, quantum) {
      beat = toDouble(beat, 'beat');
      time = toDouble(time, 'time');
      const q = this.#q(quantum);
      const currentBeat = this.beatAtTime(time);
      const phaseDelta = (((beat - currentBeat) % q) + q) % q;
      this.#timeOrigin = time;
      this.#beatOrigin = currentBeat + phaseDelta;
    }

    forceBeatAtTime(beat, time, quantum) {
      beat = toDouble(beat, 'beat');
      time = toDouble(time, 'time');
      this.#q(quantum);
      this.#timeOrigin = time;
      this.#beatOrigin = beat;
    }

    setIsPlaying(playing, atTime) {
      atTime = toDouble(atTime, 'atTime');
      this.#isPlaying = Boolean(playing);
      this.#timeForIsPlaying = atTime;
      this.#startStopBeat = null;
    }

    static {
      readState = (state, session) => {
        if (state.#session !== session) {
          throw new DOMException('This state was captured from another session', 'InvalidModificationError');
        }
        if (state.#committed) {
          throw new DOMException('This state was already committed', 'InvalidStateError');
        }
        state.#committed = true;
        const toHost = state.#toHost;
        return {
          tempo: Math.round(60e6 / state.#tempo),
          beatOrigin: Math.round(state.#beatOrigin * 1e6),
          timeOrigin: toHost(state.#timeOrigin),
          isPlaying: state.#isPlaying,
          startStopBeat: state.#startStopBeat ?? Math.round(state.beatAtTime(state.#timeForIsPlaying) * 1e6),
          startStopTime: toHost(state.#timeForIsPlaying),
        };
      };
    }
  }

  class TempoSession extends EventTarget {
    #id;
    #quantum;
    #closed = false;

    constructor(t, id, quantum) {
      if (t !== token) {
        throw illegal();
      }
      super();
      this.#id = id;
      this.#quantum = quantum;
    }

    get numPeers() {
      return numPeers;
    }

    get quantum() {
      return this.#quantum;
    }

    captureState(context = undefined) {
      if (this.#closed) {
        throw new DOMException('The session is closed', 'InvalidStateError');
      }
      if (context !== undefined && !(context instanceof BaseAudioContext)) {
        throw new TypeError('captureState() takes a BaseAudioContext');
      }
      const contextOffset = context ? audioOffset(context) : 0;
      const offset = offsetUs;
      const toLocal = (us) => (us - offset) / 1e6 - contextOffset;
      const toHost = (seconds) => Math.round((seconds + contextOffset) * 1e6 + offset);
      return new TempoSessionState(token, this, {
        tempo: 60e6 / hostState.tempo,
        beatOrigin: hostState.beatOrigin / 1e6,
        timeOrigin: toLocal(hostState.timeOrigin),
        quantum: this.#quantum,
        isPlaying: hostState.isPlaying,
        timeForIsPlaying: toLocal(hostState.startStopTime),
        startStopBeat: hostState.startStopBeat,
      }, toHost);
    }

    commitState(state) {
      if (this.#closed) {
        throw new DOMException('The session is closed', 'InvalidStateError');
      }
      if (!(state instanceof TempoSessionState)) {
        throw new TypeError('commitState() takes a TempoSessionState');
      }
      const next = readState(state, this);
      // Show the change to this document now; the host echoes it once it's applied.
      applyState(next, numPeers);
      post({ type: 'commit', state: next });
    }

    close() {
      if (this.#closed) {
        return;
      }
      this.#closed = true;
      sessions.delete(this);
      post({ type: 'close', id: this.#id });
    }

    #fire(prev, prevPeers) {
      if (prev && prev.tempo !== hostState.tempo) {
        this.dispatchEvent(new Event('tempochange'));
      }
      if (prev && prev.isPlaying !== hostState.isPlaying) {
        this.dispatchEvent(new Event('playingchange'));
      }
      if (prevPeers !== numPeers) {
        this.dispatchEvent(new Event('numpeerschange'));
      }
    }

    static {
      notifySession = (session, prev, prevPeers) => session.#fire(prev, prevPeers);
    }
  }
  for (const type of ['numpeerschange', 'tempochange', 'playingchange']) {
    defineHandler(TempoSession.prototype, type);
  }

  class TempoSync {
    constructor(t) {
      if (t !== token) {
        throw illegal();
      }
    }

    requestSession(options = {}) {
      let quantum;
      try {
        if (options !== null && typeof options !== 'object') {
          throw new TypeError('options must be a dictionary');
        }
        quantum = options?.quantum === undefined ? 4 : toDouble(options.quantum, 'quantum');
      } catch (e) {
        return Promise.reject(e);
      }
      if (quantum <= 0) {
        return Promise.reject(new RangeError('quantum must be positive'));
      }
      return new Promise((resolve, reject) => {
        const id = nextId++;
        opening.set(id, {
          resolve: () => {
            const session = new TempoSession(token, id, quantum);
            sessions.add(session);
            resolve(session);
          },
          reject,
        });
        post({ type: 'open', id });
      });
    }
  }

  window.addEventListener('message', (event) => {
    if (event.source !== window || event.data?.[TAG] !== 'ext') {
      return;
    }
    const msg = event.data.msg;
    switch (msg.type) {
      case 'state':
        applyState(msg.state, msg.numPeers);
        break;
      case 'clock':
        offsetUs = msg.offsetUs;
        break;
      case 'opened': {
        const request = opening.get(msg.id);
        opening.delete(msg.id);
        if (!request) {
          break;
        }
        if (msg.error) {
          request.reject(new DOMException(msg.error.message, msg.error.name));
        } else {
          request.resolve();
        }
        break;
      }
      case 'hostlost':
        console.warn(`navigator.tempoSync: ${msg.message}`);
        for (const request of opening.values()) {
          request.reject(new DOMException(msg.message, 'NotSupportedError'));
        }
        opening.clear();
        if (numPeers !== 0) {
          applyState(hostState, 0);
        }
        break;
    }
  });

  const tempoSync = new TempoSync(token);
  Object.defineProperty(Navigator.prototype, 'tempoSync', {
    configurable: true,
    enumerable: true,
    get() {
      return tempoSync;
    },
  });
  for (const cls of [TempoSync, TempoSession, TempoSessionState]) {
    Object.defineProperty(window, cls.name, { configurable: true, writable: true, value: cls });
  }
})();
