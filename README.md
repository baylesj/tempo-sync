# Web Tempo Sync (Ableton Link compatible)

Part of **Web Tempo Sync**: [spec and explainer](https://github.com/baylesj/web-tempo-sync) · [implementation and extension](https://github.com/baylesj/tempo-sync) · [demo](https://github.com/baylesj/web-tempo-sync-demo) · [all repos](https://github.com/search?q=topic%3Aweb-tempo-sync+user%3Abaylesj&type=repositories)

A Rust implementation of the Web Tempo Sync protocol (compatible with Ableton Link).

## Design
This repository uses a sans-IO design, separated into several crates:
- `tempo-sync-core`: The dependency-free, sans-IO protocol engine. Reusable via C++ (e.g. Chromium via cxx) or in other environments like Firefox, as it does not do any actual network I/O or reading of the system clock.
- `tempo-sync-sim`: A deterministic simulator used for asserting convergence over complex virtual network conditions.
- `tempo-sync-std`: A standard library adapter providing a monotonic clock, UDP sockets, and a background thread.
- `tempohut`: A command line tool for testing and debugging, with JSON input/output designed for interoperability testing.
- `tempo-sync-host`: A native messaging host that lets the Chrome extension in `extension/` join sessions, so pages get `navigator.tempoSync` on stable Chrome. See `extension/README.md`.

### Clean Room Rule
To ensure licensing compliance, this implementation is developed in a clean room environment. It is derived entirely from `docs/PROTOCOL.md` and `docs/TEST_VECTORS.md` and **never** reads existing GPL sources. 

## Licensing
This project is licensed under the Apache License, Version 2.0.

## Running Tests
Ensure you have Rust installed.
`cargo test`

## Running Tempohut
`cargo run -p tempohut -- --json --tempo 120 --quantum 4 --interval-ms 100 --iface 127.0.0.1`
