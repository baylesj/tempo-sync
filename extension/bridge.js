// Carries messages between page.js and the service worker, and measures the offset between
// this document's clock and the host's.
//
// A content script shares the page's performance timeline, so pinging from here times the
// page clock without adding the window.postMessage hop. Each ping still crosses the service
// worker and Chrome's pipe to the host, so the offset is an estimate: the sample with the
// shortest round trip in the last WINDOW_MS wins, since it had the least room for asymmetry.

const TAG = '__webTempoSync';
const WINDOW_MS = 20000;
const BURST = 8;
const BURST_GAP_MS = 10;
const RESYNC_MS = 1000;

let port = null;
const sessions = new Set();
let samples = [];
let offsetUs = null;
let resyncTimer = null;
let clockReady = null;
let nextPing = 1;
const pings = new Map();

function toPage(msg) {
  window.postMessage({ [TAG]: 'ext', msg }, '*');
}

function toWorker(msg) {
  if (!port) {
    port = chrome.runtime.connect({ name: 'tempo-sync' });
    port.onMessage.addListener(onWorkerMessage);
    port.onDisconnect.addListener(() => {
      port = null;
      stopClock();
      if (sessions.size) {
        sessions.clear();
        toPage({ type: 'hostlost', message: 'The Web Tempo Sync extension was reloaded or turned off.' });
      }
    });
  }
  port.postMessage(msg);
}

function ping() {
  const id = nextPing++;
  pings.set(id, performance.now());
  toWorker({ type: 'ping', id });
}

function onPong({ id, hostTime }) {
  const t1 = performance.now();
  const t0 = pings.get(id);
  pings.delete(id);
  if (t0 === undefined) {
    return;
  }
  samples.push({ at: t1, rttUs: (t1 - t0) * 1000, offsetUs: hostTime - ((t0 + t1) / 2) * 1000 });
  samples = samples.filter((s) => t1 - s.at < WINDOW_MS);
  const best = samples.reduce((a, b) => (b.rttUs < a.rttUs ? b : a));
  if (best.offsetUs !== offsetUs) {
    offsetUs = best.offsetUs;
    toPage({ type: 'clock', offsetUs });
  }
  if (samples.length >= BURST && clockReady) {
    clockReady.resolve();
  }
}

function burst() {
  for (let i = 0; i < BURST; i++) {
    setTimeout(ping, i * BURST_GAP_MS);
  }
}

function sendClockStats() {
  if (!samples.length) {
    return;
  }
  const rtts = samples.map((s) => s.rttUs).sort((a, b) => a - b);
  toWorker({ type: 'clockstats', minRttUs: rtts[0], medianRttUs: rtts[rtts.length >> 1], samples: rtts.length });
}

// Resolves once the burst has given the page an offset to work with.
function startClock() {
  if (!clockReady) {
    let resolve;
    clockReady = new Promise((r) => (resolve = r));
    clockReady.resolve = resolve;
    burst();
    resyncTimer = setInterval(() => {
      burst();
      setTimeout(sendClockStats, BURST * BURST_GAP_MS + 50);
    }, RESYNC_MS);
  }
  return clockReady;
}

// The host's clock starts over whenever the host process does, so forget everything.
function stopClock() {
  clearInterval(resyncTimer);
  resyncTimer = null;
  clockReady = null;
  samples = [];
  offsetUs = null;
  pings.clear();
}

async function onWorkerMessage(msg) {
  switch (msg.type) {
    case 'pong':
      onPong(msg);
      break;
    case 'state':
    case 'sessions':
      toPage(msg);
      break;
    case 'opened':
      if (!msg.error) {
        sessions.add(msg.id);
        await startClock();
      }
      toPage(msg);
      break;
    case 'hostlost':
      sessions.clear();
      stopClock();
      toPage(msg);
      break;
  }
}

window.addEventListener('message', (event) => {
  if (event.source !== window || event.data?.[TAG] !== 'page') {
    return;
  }
  const msg = event.data.msg;
  switch (msg.type) {
    case 'open':
      toWorker({ type: 'open', id: msg.id });
      break;
    case 'close':
      sessions.delete(msg.id);
      toWorker({ type: 'close', id: msg.id });
      if (!sessions.size) {
        stopClock();
      }
      break;
    case 'commit':
      toWorker({ type: 'commit', state: msg.state });
      break;
  }
});
