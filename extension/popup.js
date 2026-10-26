const $ = (id) => document.getElementById(id);

async function render() {
  const status = await chrome.runtime.sendMessage({ type: 'status' });
  const sites = new Set(status.sessions.map((s) => s.origin));
  $('helper').textContent = status.running ? `running for ${sites.size} site${sites.size === 1 ? '' : 's'}` : 'idle';
  $('helper').className = status.running ? 'ok' : '';

  const s = status.state;
  $('peers').textContent = s ? String(s.numPeers) : '–';
  $('tempo').textContent = s ? `${(60e6 / s.state.tempo).toFixed(2)} BPM` : '–';
  $('transport').textContent = s ? (s.state.isPlaying ? 'playing' : 'stopped') : '–';

  // Round trips from each page to the helper: the best one bounds the clock offset's error.
  const clocks = status.sessions.map((x) => x.clock).filter(Boolean);
  $('clock').textContent = clocks.length
    ? clocks.map((c) => `rtt ${(c.minRttUs / 1000).toFixed(2)} ms best, ${(c.medianRttUs / 1000).toFixed(2)} median`).join('\n')
    : '–';

  const list = $('grants');
  list.replaceChildren();
  const entries = Object.entries(status.grants);
  if (!entries.length) {
    const li = document.createElement('li');
    li.textContent = 'No sites have asked yet.';
    list.append(li);
  }
  for (const [origin, value] of entries) {
    const li = document.createElement('li');
    const name = document.createElement('span');
    name.className = 'mono';
    name.textContent = origin;
    const right = document.createElement('span');
    right.className = 'row';
    const state = document.createElement('span');
    state.className = value === 'granted' ? 'ok' : 'warn';
    state.textContent = value === 'granted' ? 'allowed' : 'blocked';
    const reset = document.createElement('button');
    reset.className = 'link';
    reset.textContent = 'reset';
    reset.addEventListener('click', async () => {
      await chrome.runtime.sendMessage({ type: 'setGrant', origin, value: null });
      render();
    });
    right.append(state, reset);
    li.append(name, right);
    list.append(li);
  }
}

render();
setInterval(render, 500);
