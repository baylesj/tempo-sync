# Build prototype Chromium v0.2.0 (Web Tempo Sync)

You're building the next prototype Chromium for Web Tempo Sync, a proposed web API that lets a page play in time with Ableton Link apps. The patch is written and applies cleanly, but it **has never been compiled**. Your job: apply it, compile and fix it, test it, package a macOS arm64 build, and hand it back.

This is Jordan Bayles's personal prototype. Keep it out of Chromium's Gerrit: no `git cl upload`, no CLs, no try jobs. Build locally.

## Background

- **The spec:** https://bayl.es/web-tempo-sync/. The explainer is at `explainer.html` next to it.
- **The demo site** that uses the API: https://bayl.es/tempo-sync/.
- **The patch:** `patches/0001-chromium-web-tempo-sync.patch` in this repo (`baylesj/tempo-sync`). It's one squashed commit that adds:
  - Blink: `third_party/blink/renderer/modules/tempo_sync/`, plus its mojom;
  - the browser-process host and engine: `content/browser/tempo_sync/`;
  - the Rust core, vendored in `third_party/tempo_sync_core/`.
- **Changes since v0.1:** the patch's commit message lists them.
- **Reference implementations for the semantics:** `extension/page.js` in this repo, and the demo's `js/stand-in.js`, implement the same draft in JavaScript, with tests (`extension/test/page.test.mjs`). If the C++ and the JavaScript disagree, the spec decides, then the JavaScript.
- **Verified:** the patch applies cleanly to Chromium `main` at `07a619db84eff5106b44ef7e30e9a322e104fb11` (2026-10-26). A newer `main` will probably work too.

## 1. Get the patch

```sh
gh repo clone baylesj/tempo-sync ~/tempo-sync   # private: needs Jordan's GitHub account
```

If this machine can't reach that account, stop and ask Jordan. Don't fetch the patch some other way.

## 2. Apply it

In your Chromium `src` checkout:

```sh
git fetch origin && git checkout -b tempo-sync-v0.2 07a619db84eff5106b44ef7e30e9a322e104fb11   # or origin/main
gclient sync -D
git am --whitespace=nowarn ~/tempo-sync/patches/0001-chromium-web-tempo-sync.patch
```

Use Jordan's chromium.org identity for this commit; the patch's author already is `jophba@chromium.org`.

## 3. Build and fix

```sh
gn gen out/TempoSync --args='is_debug=false is_component_build=false symbol_level=0 target_cpu="arm64" dcheck_always_on=false'
autoninja -C out/TempoSync chrome content_shell
```

Fix whatever the compiler finds, keeping each fix minimal and the behaviour as the spec says. These are the places I couldn't verify without a checkout:

- **`TempoSession::QueueEvent`** uses `EventTarget::EnqueueEvent(Event&, TaskType)`. If that's not the current API, post a task on `GetExecutionContext()->GetTaskRunner(TaskType::kMiscPlatformAPI)` that calls `DispatchEvent`, so events still arrive after `commitState()` returns.
- **`TempoSessionState`'s overloads** (`beatAtTime(time)` and `beatAtTime(time, quantum)`, and so on) assume the bindings call the shorter one when the optional `quantum` is omitted, as v0.1's did. If the generator now passes `std::optional<double>`, take that instead. "Omitted or ≤ 0" means "use the session's quantum".
- **`TempoSessionHost`** uses `ReceiverSet<TempoSession, RemoteSetElementId>`, reading `current_context()` both in `Close()` and in the receiver disconnect handler. If the context isn't available in the disconnect handler, look the client up by `current_receiver()` instead, with the `receiver_for_client_` map inverted.
- **`TempoSessionState`** now holds a `Member<TempoSession>` and overrides `Trace`.
- **Rust:** `third_party/tempo_sync_core/src/` was refreshed from the crate in this repo (`crates/tempo-sync-core/src/`); `rust_tempo_sync_bridge.rs` is unchanged. If the bridge doesn't compile against it, fix the bridge, not the core.

If something in the design turns out not to work in Chromium (not just a compile error), make the smallest change that keeps the spec's behaviour, and write down what you changed and why.

## 4. Test

Automated, with the fake engine (no network):

```sh
third_party/blink/tools/run_web_tests.py -t TempoSync wpt_internal/tempo_sync/tempo-session.html \
  --additional-driver-flag=--enable-blink-features=TempoSync \
  --additional-driver-flag=--tempo-sync-engine=fake
```

All 7 subtests should pass. If a subtest is wrong rather than the code, fix the test, and say so in your report.

End to end, with real Link traffic **on a private port**, so Jordan's Ableton Live (which listens on 20808) isn't disturbed:

```sh
cd ~/tempo-sync && cargo build --release -p tempohut
PORT=21$((RANDOM % 900 + 100))
./target/release/tempohut --port $PORT --tempo 133 &
out/TempoSync/Chromium.app/Contents/MacOS/Chromium --user-data-dir=/tmp/tempo-sync-v02 \
  --enable-blink-features=TempoSync --tempo-sync-port=$PORT "https://bayl.es/tempo-sync/#sampler"
```

On the site, check each of these:

- Click **Connect**. The session bar shows **Playing along** with the tag **`browser`**. It must not say `browser v0.1`, which means the site's adapter for the old API kicked in.
- The tempo shows 133.
- **Play** starts `tempohut` on a bar line, and changing the tempo on the page changes `tempohut`'s, and vice versa: type `tempo 140` into `tempohut`'s terminal.
- The strudel, hydra, tone.js and arpeggios tabs load and play with no console errors.
- **New window** opens a second window. Both show "Playing along", and starting the sampler in one doesn't move the other's bar lines.

## 5. Update the patch in this repo

Squash any fixes into the one commit (`git commit --amend`, keeping the message and adding a line or two about the fixes), then:

```sh
git format-patch -1 --stdout HEAD > ~/tempo-sync/patches/0001-chromium-web-tempo-sync.patch
cd ~/tempo-sync && git add patches && git commit -m "Chromium patch v0.2: <what you fixed>" && git push
```

That commit, to Jordan's GitHub repo, uses his gmail identity (bayles.jordan@gmail.com), not chromium.org. Don't add a Co-Authored-By trailer.

## 6. Package

```sh
cd out/TempoSync
codesign --force --deep -s - Chromium.app
ditto -c -k --keepParent Chromium.app Chromium-tempo-sync-v0.2.0-macos-arm64.zip
shasum -a 256 Chromium-tempo-sync-v0.2.0-macos-arm64.zip > Chromium-tempo-sync-v0.2.0-macos-arm64.zip.sha256
```

The build is signed ad hoc only, so users run `xattr -cr Chromium.app` first, as with v0.1.

## 7. Hand it back

If `gh` on this machine can reach Jordan's account, attach both files to a release:

```sh
gh release create v0.2.0 -R baylesj/web-tempo-sync-demo \
  --title "Prototype Chromium v0.2.0" \
  --notes "Prototype Chromium (macOS, Apple silicon) implementing the current Web Tempo Sync draft: hasPeers, timelinechange, per-page beat numbering. Run with --enable-blink-features=TempoSync." \
  out/TempoSync/Chromium-tempo-sync-v0.2.0-macos-arm64.zip out/TempoSync/Chromium-tempo-sync-v0.2.0-macos-arm64.zip.sha256
```

Otherwise leave the two files somewhere Jordan can get them and tell him where. Jordan's home setup mirrors the release to bayl.es and updates the demo's setup tab; you don't need to.

## Report

Reply with:

- the Chromium revision you built on;
- each fix you made, in a line or two;
- the web test results;
- what you saw in the end-to-end checks;
- the `tempo-sync` commit hash, and the release URL (or file location).
