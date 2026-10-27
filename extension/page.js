// navigator.tempoSync, following the Web Tempo Sync draft (https://baylesj.github.io/web-tempo-sync/).
//
// Runs in the page's own world so pages get ordinary objects. bridge.js, in the extension's
// isolated world, carries messages to the service worker and keeps the clock offset.
//
// The host keeps the shared timeline as integers in its own clock: microseconds, micro-beats
// and microseconds per beat. They're converted to page seconds at captureState() and back at
// commitState(). Each session's beat offset (its own beat numbering) never leaves the page.
//
// Not polyfilled: TempoSessionState isn't [Serializable] here, since an extension can't reach
// a page's workers or worklets.
(() => {
  if (!window.isSecureContext || 'tempoSync' in Navigator.prototype) {
    return;
  }

  const TAG = '__webTempoSync';
  const token = Symbol('internal');
  const illegal = () => new TypeError('Illegal constructor');

  let hostState = null;
  let networkPeers = 0;
  // Open sessions in the whole browser, this document's included.
  let browserSessions = 0;
  // host µs = performance.now() * 1000 + offsetUs, measured by bridge.js.
  let offsetUs = 0;
  const sessions = new Set();
  const opening = new Map();
  let nextId = 1;

  // Set inside the classes' static blocks, so module code can reach their private fields.
  let notifySession;
  let readState;

  const post = (msg) => window.postMessage({ [TAG]: 'page', msg }, '*');
  const hasOthers = () => networkPeers > 0 || browserSessions > 1;

  // WebIDL `double`: anything that converts to a non-finite number is a TypeError.
  function toDouble(value, name) {
    const n = Number(value);
    if (!Number.isFinite(n)) {
      throw new TypeError(`${name} is not a finite number`);
    }
    return n;
  }

  const phase = (beat, q) => ((beat % q) + q) % q;

  // The beat offset rounded to a multiple of q, halves going up, as the draft defines it.
  const offsetFor = (offset, q) => q * Math.floor(offset / q + 0.5);

  // Two host timelines are the same line if they agree on tempo and on the beat at one time.
  // The host re-anchors a committed timeline, so comparing fields alone would see changes
  // that aren't there.
  function sameTimeline(a, b) {
    if (a.tempo !== b.tempo) {
      return false;
    }
    const beatAtB = a.beatOrigin + ((b.timeOrigin - a.timeOrigin) / a.tempo) * 1e6;
    return Math.abs(beatAtB - b.beatOrigin) <= 2;
  }

  // AudioContext time = page seconds - audioOffset(context). getOutputTimestamp() pairs the
  // context time now reaching the speakers with the page time it got there, so this maps to
  // output time. It steps by a render quantum, so follow it slowly unless it jumps (device
  // change, suspend and resume).
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

  // Events are queued, as the draft says, so a commit's own events arrive after it returns.
  function applyHostUpdate({ state = hostState, peers = networkPeers, count = browserSessions }) {
    const before = { state: hostState, others: hasOthers() };
    hostState = state;
    networkPeers = peers;
    browserSessions = count;
    const changes = {
      timeline: before.state && !sameTimeline(before.state, hostState),
      playing: before.state && (before.state.isPlaying !== hostState.isPlaying
        || before.state.startStopTime !== hostState.startStopTime),
      peers: before.others !== hasOthers(),
    };
    if (changes.timeline || changes.playing || changes.peers) {
      const targets = [...sessions];
      setTimeout(() => targets.forEach((s) => notifySession(s, changes)));
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
    #timeOrigin;
    #beatOrigin;
    #offset;
    #quantum;
    #playing;
    #playingTime;
    #solo;
    #timelineChanged = false;
    #playingChanged = false;
    #committed = false;

    constructor(t, session, fields, toHost) {
      if (t !== token) {
        throw illegal();
      }
      this.#session = session;
      this.#toHost = toHost;
      this.#tempo = fields.tempo;
      this.#timeOrigin = fields.timeOrigin;
      this.#beatOrigin = fields.beatOrigin;
      this.#offset = fields.offset;
      this.#quantum = fields.quantum;
      this.#playing = fields.playing;
      this.#playingTime = fields.playingTime;
      this.#solo = fields.solo;
    }

    #q(quantum) {
      if (quantum === undefined) {
        return this.#quantum;
      }
      const q = toDouble(quantum, 'quantum');
      return q > 0 ? q : this.#quantum;
    }

    #sharedBeat(time) {
      return this.#beatOrigin + (time - this.#timeOrigin) * (this.#tempo / 60);
    }

    #sharedTime(beat) {
      return this.#timeOrigin + (beat - this.#beatOrigin) * (60 / this.#tempo);
    }

    get tempo() {
      return this.#tempo;
    }

    get isPlaying() {
      return this.#playing;
    }

    get timeForIsPlaying() {
      return this.#playingTime;
    }

    beatAtTime(time, quantum) {
      time = toDouble(time, 'time');
      return this.#sharedBeat(time) + offsetFor(this.#offset, this.#q(quantum));
    }

    timeAtBeat(beat, quantum) {
      beat = toDouble(beat, 'beat');
      return this.#sharedTime(beat - offsetFor(this.#offset, this.#q(quantum)));
    }

    phaseAtTime(time, quantum) {
      time = toDouble(time, 'time');
      return phase(this.#sharedBeat(time), this.#q(quantum));
    }

    setTempo(bpm, atTime) {
      bpm = toDouble(bpm, 'bpm');
      atTime = toDouble(atTime, 'atTime');
      if (bpm <= 0) {
        throw new RangeError('bpm must be positive');
      }
      this.#beatOrigin = this.#sharedBeat(atTime);
      this.#timeOrigin = atTime;
      this.#tempo = bpm;
      this.#timelineChanged = true;
    }

    // Alone, put `beat` at `time`. With others, wait for the next point where the session's
    // phase matches `beat`'s and number it `beat`, changing only this session's numbering.
    requestBeatAtTime(beat, time, quantum) {
      beat = toDouble(beat, 'beat');
      time = toDouble(time, 'time');
      const q = this.#q(quantum);
      if (this.#solo) {
        this.forceBeatAtTime(beat, time, q);
        return;
      }
      const current = this.beatAtTime(time, q);
      const launch = current + phase(phase(beat, q) - phase(current, q), q);
      this.#offset = beat - this.#sharedBeat(this.timeAtBeat(launch, q));
    }

    // Moves the bar lines for everyone, by at most half a quantum.
    forceBeatAtTime(beat, time, quantum) {
      beat = toDouble(beat, 'beat');
      time = toDouble(time, 'time');
      const q = this.#q(quantum);
      const current = this.#sharedBeat(time);
      let shift = phase(phase(beat, q) - phase(current, q), q);
      if (shift > q / 2) {
        shift -= q;
      }
      this.#timeOrigin = time;
      this.#beatOrigin = current + shift;
      this.#offset = beat - (current + shift);
      this.#timelineChanged = true;
    }

    setIsPlaying(playing, atTime) {
      atTime = toDouble(atTime, 'atTime');
      this.#playing = Boolean(playing);
      this.#playingTime = atTime;
      this.#playingChanged = true;
    }

    static {
      // What commitState() needs: the new beat offset, and the host state to share, if the
      // state changed any of it. Unchanged parts come from the latest host state, so a stale
      // snapshot doesn't undo another participant's change.
      readState = (state, session) => {
        if (state.#session !== session) {
          throw new DOMException('This state was captured from another session', 'InvalidModificationError');
        }
        if (state.#committed) {
          throw new DOMException('This state was already committed', 'InvalidStateError');
        }
        state.#committed = true;
        if (!state.#timelineChanged && !state.#playingChanged) {
          return { offset: state.#offset, share: null };
        }
        const toHost = state.#toHost;
        const share = { ...hostState };
        if (state.#timelineChanged) {
          share.tempo = Math.round(60e6 / state.#tempo);
          share.beatOrigin = Math.round(state.#beatOrigin * 1e6);
          share.timeOrigin = toHost(state.#timeOrigin);
        }
        if (state.#playingChanged) {
          share.isPlaying = state.#playing;
          share.startStopTime = toHost(state.#playingTime);
          share.startStopBeat = Math.round(state.#sharedBeat(state.#playingTime) * 1e6);
        }
        return { offset: state.#offset, share };
      };
    }
  }

  class TempoSession extends EventTarget {
    #id;
    #quantum;
    #offset = 0;
    #closed = false;

    constructor(t, id, quantum) {
      if (t !== token) {
        throw illegal();
      }
      super();
      this.#id = id;
      this.#quantum = quantum;
    }

    get hasPeers() {
      return hasOthers();
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
        timeOrigin: toLocal(hostState.timeOrigin),
        beatOrigin: hostState.beatOrigin / 1e6,
        offset: this.#offset,
        quantum: this.#quantum,
        playing: hostState.isPlaying,
        playingTime: toLocal(hostState.startStopTime),
        solo: !hasOthers(),
      }, toHost);
    }

    commitState(state) {
      if (this.#closed) {
        throw new DOMException('The session is closed', 'InvalidStateError');
      }
      if (!(state instanceof TempoSessionState)) {
        throw new TypeError('commitState() takes a TempoSessionState');
      }
      const { offset, share } = readState(state, this);
      this.#offset = offset;
      if (share) {
        // Show the change in this document now; the host echoes it once it's applied.
        applyHostUpdate({ state: share });
        post({ type: 'commit', state: share });
      }
    }

    close() {
      if (this.#closed) {
        return;
      }
      this.#closed = true;
      sessions.delete(this);
      post({ type: 'close', id: this.#id });
    }

    #fire(changes) {
      if (this.#closed) {
        return;
      }
      if (changes.timeline) {
        this.dispatchEvent(new Event('timelinechange'));
      }
      if (changes.playing) {
        this.dispatchEvent(new Event('playingchange'));
      }
      if (changes.peers) {
        this.dispatchEvent(new Event('peerschange'));
      }
    }

    static {
      notifySession = (session, changes) => session.#fire(changes);
    }
  }
  for (const type of ['peerschange', 'timelinechange', 'playingchange']) {
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
        applyHostUpdate({ state: msg.state, peers: msg.numPeers });
        break;
      case 'sessions':
        applyHostUpdate({ count: msg.count });
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
        if (hostState) {
          applyHostUpdate({ peers: 0, count: sessions.size });
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
