const origin = new URLSearchParams(location.search).get('origin');
document.getElementById('origin').textContent = origin;

const decide = (allow) => chrome.runtime.sendMessage({ type: 'decision', origin, allow });
document.getElementById('allow').addEventListener('click', () => decide(true));
document.getElementById('block').addEventListener('click', () => decide(false));
document.getElementById('allow').focus();

// Keep the service worker, which is waiting on this answer, from being stopped as idle.
setInterval(() => chrome.runtime.sendMessage({ type: 'heartbeat' }), 10000);
