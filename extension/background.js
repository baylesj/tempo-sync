// Owns the connection to tempo-sync-host and the per-origin "tempo-sync" permission.
//
// The host runs only while some document has a session open: connecting starts it, and
// disconnecting closes its stdin, which makes it say goodbye to its peers and exit. An open
// native port also keeps this service worker alive.

const HOST = 'es.bayl.tempo_sync';
const HOST_MISSING = 'tempo-sync-host is not installed. Run extension/install-host.sh from the tempo-sync repo, then reload the page.';

let native = null;
let ready = null;
let lastState = null;
let nextClient = 1;
const clients = new Map();
const prompts = new Map();

function anySessions() {
  return [...clients.values()].some((c) => c.sessions.size);
}

// Pages count other tabs' sessions as participants (for hasPeers, and so starting a loop in
// one tab waits for the bar line instead of moving another tab's), so they need the total.
function broadcastSessions() {
  const count = [...clients.values()].reduce((n, c) => n + c.sessions.size, 0);
  for (const c of clients.values()) {
    if (c.sessions.size) {
      c.port.postMessage({ type: 'sessions', count });
    }
  }
}

function startHost() {
  if (ready) {
    return ready;
  }
  ready = new Promise((resolve, reject) => {
    native = chrome.runtime.connectNative(HOST);
    native.onMessage.addListener((msg) => {
      if (msg.type === 'state') {
        lastState = msg;
        resolve();
        for (const c of clients.values()) {
          if (c.sessions.size) {
            c.port.postMessage(msg);
          }
        }
      } else if (msg.type === 'pong') {
        const [clientId, id] = msg.id;
        clients.get(clientId)?.port.postMessage({ type: 'pong', id, hostTime: msg.hostTime });
      }
    });
    native.onDisconnect.addListener(() => {
      const error = chrome.runtime.lastError?.message ?? '';
      const message = /not found/i.test(error) ? HOST_MISSING : `tempo-sync-host stopped. ${error}`.trim();
      native = null;
      ready = null;
      lastState = null;
      reject(new Error(message));
      for (const c of clients.values()) {
        if (c.sessions.size) {
          c.sessions.clear();
          c.port.postMessage({ type: 'hostlost', message });
        }
      }
    });
  });
  return ready;
}

function stopHostIfIdle() {
  if (native && !anySessions()) {
    native.disconnect();
    native = null;
    ready = null;
    lastState = null;
  }
}

async function getGrants() {
  return (await chrome.storage.local.get('grants')).grants ?? {};
}

async function setGrant(origin, value) {
  const grants = await getGrants();
  if (value) {
    grants[origin] = value;
  } else {
    delete grants[origin];
  }
  await chrome.storage.local.set({ grants });
}

// Resolves to 'granted', 'denied', or 'dismissed' (window closed without choosing, which
// isn't remembered). Requests from several tabs of one origin share a prompt.
function requestPermission(origin, tab) {
  if (!prompts.has(origin)) {
    prompts.set(origin, showPrompt(origin, tab).finally(() => prompts.delete(origin)));
  }
  return prompts.get(origin);
}

async function showPrompt(origin, tab) {
  const width = 400;
  const height = 230;
  const opener = tab ? await chrome.windows.get(tab.windowId).catch(() => null) : null;
  const win = await chrome.windows.create({
    url: `prompt.html?origin=${encodeURIComponent(origin)}`,
    type: 'popup',
    width,
    height,
    left: opener ? Math.round(opener.left + (opener.width - width) / 2) : undefined,
    top: opener ? opener.top + 80 : undefined,
    focused: true,
  });
  return new Promise((resolve) => {
    const onMessage = (msg, sender) => {
      if (msg.type === 'decision' && msg.origin === origin && sender.tab?.windowId === win.id) {
        finish(msg.allow ? 'granted' : 'denied');
      }
    };
    const onRemoved = (windowId) => {
      if (windowId === win.id) {
        finish('dismissed');
      }
    };
    const finish = async (decision) => {
      chrome.runtime.onMessage.removeListener(onMessage);
      chrome.windows.onRemoved.removeListener(onRemoved);
      if (decision !== 'dismissed') {
        await setGrant(origin, decision);
        chrome.windows.remove(win.id).catch(() => {});
      }
      resolve(decision);
    };
    chrome.runtime.onMessage.addListener(onMessage);
    chrome.windows.onRemoved.addListener(onRemoved);
  });
}

async function open(client, id) {
  const reply = (error) => client.port.postMessage({ type: 'opened', id, error });
  let decision = (await getGrants())[client.origin];
  if (!decision) {
    decision = await requestPermission(client.origin, client.port.sender.tab);
  }
  if (decision !== 'granted') {
    return reply({ name: 'NotAllowedError', message: 'Permission to use tempo-sync was denied.' });
  }
  try {
    await startHost();
  } catch (e) {
    return reply({ name: 'NotSupportedError', message: e.message });
  }
  // The page may have gone, or the host stopped, while we waited on the prompt or the host.
  if (!clients.has(client.id)) {
    return stopHostIfIdle();
  }
  if (!lastState) {
    return reply({ name: 'NotSupportedError', message: 'tempo-sync-host stopped.' });
  }
  client.sessions.add(id);
  client.port.postMessage(lastState);
  broadcastSessions();
  reply();
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== 'tempo-sync' || !port.sender?.origin) {
    return;
  }
  const client = { id: nextClient++, port, origin: port.sender.origin, sessions: new Set(), clock: null };
  clients.set(client.id, client);
  port.onMessage.addListener((msg) => {
    switch (msg.type) {
      case 'open':
        open(client, msg.id);
        break;
      case 'close':
        client.sessions.delete(msg.id);
        broadcastSessions();
        stopHostIfIdle();
        break;
      case 'ping':
        native?.postMessage({ type: 'ping', id: [client.id, msg.id] });
        break;
      case 'commit':
        if (client.sessions.size) {
          native?.postMessage({ type: 'commit', state: msg.state });
        }
        break;
      case 'clockstats':
        client.clock = msg;
        break;
    }
  });
  port.onDisconnect.addListener(() => {
    clients.delete(client.id);
    broadcastSessions();
    stopHostIfIdle();
  });
});

// For the action popup and the permission prompt.
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  switch (msg.type) {
    case 'status':
      getGrants().then((grants) => sendResponse({
        running: Boolean(native),
        state: lastState,
        grants,
        sessions: [...clients.values()]
          .filter((c) => c.sessions.size)
          .map((c) => ({ origin: c.origin, tabId: c.port.sender.tab?.id, clock: c.clock })),
      }));
      return true;
    case 'setGrant':
      setGrant(msg.origin, msg.value).then(() => sendResponse());
      return true;
    case 'heartbeat':
      // The prompt calls in while it's open so this worker isn't stopped while we wait.
      sendResponse();
      return false;
  }
  return false;
});
