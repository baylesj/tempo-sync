# Questions for Protocol Author

1. **Endpoint Entries**: What is the wire layout (key and size) and payload format for `mep4`, `mep6`, `aep4`, `aep6` entries?
   * **Answer**: 
     - IPv4 Endpoints (`mep4`, `aep4`): Key is the string itself (`0x6d657034`, `0x61657034`). Size is 6 bytes. Payload is 4 bytes of IPv4 Address (Big Endian) followed by 2 bytes of Port (Big Endian).
     - IPv6 Endpoints (`mep6`, `aep6`): Key is the string itself (`0x6d657036`, `0x61657036`). Size is 18 bytes. Payload is 16 bytes of IPv6 Address followed by 2 bytes of Port (Big Endian).
     - Note: If an endpoint is empty/unconfigured, Link completely omits the entire payload entry (meaning it writes 0 bytes and does not even write the 8-byte header for that entry).

2. **Clock Domains**: For `__ht`, `__gt`, `_pgt`, `TimeOrigin`, and `StartStopState.Timestamp`, which clock domain is each explicitly in (HostTime of sender, or GHostTime)?
   * **Answer**: 
     - `tmln.TimeOrigin` and `stst.Timestamp` are explicitly in **Ghost Time** (the common session clock).
     - In Ping messages, `__ht` is the **Host Time** of the pinger.
     - In Pong messages, `__gt` is the **Ghost Time** of the responder.
     - `_pgt` (which is copied from previous pongs) is the **Ghost Time** of the responder from the previous pong exchange.

3. **Session Merge Rules**: What is the precise rule for merging `StartStopState` when adopting a session? Does it just take the maximum timestamp, or does the timeline offset adjust the timestamp?
   * **Answer**: It is a direct timestamp comparison in **Ghost Time**. A peer compares the incoming `stst.Timestamp` against its current `mSessionState.startStopState.timestamp`. If the incoming timestamp is strictly greater, it replaces the local session state. When a local application triggers a transport change, the peer converts the local Host Time of the event into Ghost Time (using the session's `GhostXForm`) and uses that as the new `Timestamp`.

4. **Measurement Constants**: What is the interval between pings? Is there a "fast measurement" phase when a peer is first discovered?
   * **Answer**: 
     - The interval between *unanswered* pings (the measurement timeout) is **50 milliseconds**.
     - However, when a peer receives a `Pong`, it *immediately* fires the next `Ping` and resets the 50ms timer.
     - A measurement cycle attempts to collect 100 data points (which takes ~51 ping/pong exchanges, as each pair after the first yields 2 points). It finishes early if it reaches 100 points, or fails if the 50ms timer fires 5 times cumulatively across the entire measurement cycle (`kNumberMeasurements = 5`). 
     - There is no separate "fast measurement" mode; a new session immediately triggers this tight ping/pong loop, and then re-measures periodically every 30 seconds.

5. **Timeline Entry Field Order**: Is the payload for `tmln` strictly `(tempo, timeOrigin, beatOrigin)` or `(tempo, beatOrigin, timeOrigin)`?
   * **Answer**: It is strictly **`tempo, beatOrigin, timeOrigin`**. Confirmed via the C++ `Timeline` serializer (`std::tie(tl.tempo, tl.beatOrigin, tl.timeOrigin)`) and the hex dump in `TEST_VECTORS.md`.

6. **Pong composition**: Does the pong message wrap the ping inside another message header, or is it flat?
   * **Answer**: It is flat. The Pong message consists of a SINGLE 9-byte header (`_link_v1` + `0x02`), followed by the Pong's own payload entries (`sess`, `__gt`), followed immediately by the *payload entries* of the Ping it received (`__ht` and `_pgt`). It does **not** append the Ping's 9-byte message header.
