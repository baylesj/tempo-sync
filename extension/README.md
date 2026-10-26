# Web Tempo Sync extension

A Chrome extension that gives pages `navigator.tempoSync`, as drafted in the
[Web Tempo Sync spec](https://bayl.es/web-tempo-sync/), on stable Chrome. Pages can't open
UDP sockets and neither can extensions, so a small native helper, `tempo-sync-host` (in
`crates/`), joins Link sessions for it over Chrome's native messaging.

This is a prototype for trying the API and measuring what an extension can't do as well as a
browser can. It isn't a way to ship the feature.

## Install

1. Build and register the helper (macOS and Linux):

   ```sh
   extension/install-host.sh
   ```

   This builds `target/release/tempo-sync-host` and writes `es.bayl.tempo_sync.json` into the
   `NativeMessagingHosts` directory of each installed Chrome and Chromium.
   `install-host.sh --uninstall` removes those files.
2. Open `chrome://extensions`, turn on Developer mode, choose **Load unpacked**, and pick this
   directory. The `key` in `manifest.json` pins the extension ID to
   `ddfdmjnclbpgiijdcbjbginpnblceeep`, which is what the helper's manifest allows.
3. Open a page that uses the API, such as the
   [tempo sync demo](https://bayl.es/tempo-sync-demo/). The first `requestSession()` from a
   site opens a prompt. The toolbar popup shows the session and the sites you've allowed or
   blocked.

## How it fits together

```
page.js (page's world)  ⇄ postMessage ⇄  bridge.js (content script)  ⇄ port ⇄  background.js  ⇄ stdio ⇄  tempo-sync-host  ⇄ UDP ⇄  Link peers
```

- **page.js** defines `TempoSync`, `TempoSession` and `TempoSessionState`, following the
  draft's IDL and algorithms. The beat and phase maths runs in the page, from a snapshot.
- **bridge.js** relays messages and estimates the offset between the page's
  `performance.now()` and the helper's clock. It shares the page's performance timeline, so it
  can time pings without the `postMessage` hop.
- **background.js** owns the native port and the per-origin permission. It starts the helper
  for the first open session and stops it after the last one closes, so the helper says
  goodbye to its peers. One helper process serves every tab, so the machine is one Link peer.
- **tempo-sync-host** wraps `tempo_sync_std::Runtime`. Session times cross as integers in the
  runtime's own units, so a state that is captured and committed unchanged round-trips exactly.

## What the extension can't do as well as a browser

- **Clock mapping is estimated.** In the Chromium prototype the engine and
  `performance.now()` share `base::TimeTicks`, so the conversion is plain arithmetic. Here
  every ping crosses the service worker and a pipe, and `performance.now()` is coarsened to
  100 µs on pages that aren't cross-origin isolated. On an M-series Mac, headless, the best
  round trip was 0.3 ms and the median about 2 ms. The offset comes from the best round trip
  in the last 20 s, so its error is at most half of that, plus the coarsening. The popup shows
  the live figures.
- **The audio clock is estimated twice:** once to the page's clock (above), and again to the
  `AudioContext` through `getOutputTimestamp()`, smoothed in `page.js`.
- **Installing the helper is a separate step** that most people won't take, and native
  messaging isn't available on Android or ChromeOS at all.
- **More access than the feature needs.** The content scripts run on every site, and the
  helper runs outside the browser's sandbox.
- **No Permissions Policy.** Chrome doesn't know the `"tempo-sync"` feature, so the API is
  only added to top-level documents, never to iframes, rather than to iframes that
  `allow="tempo-sync"`.
- **No integration with Chrome's own permission UI** or Local Network Access.

## Differences from the draft

- `TempoSessionState` isn't serializable: an extension can't reach a page's workers or
  worklets, so snapshots can't be posted to them.

- `startStopSync` is accepted and ignored, as in the Chromium prototype.
- `commitState()` isn't rate limited yet.

## Tests

```sh
node --test extension/test/*.test.mjs   # page.js against a stand-in bridge
cargo test -p tempo-sync-host           # framing and state conversion
```
