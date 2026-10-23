#!/bin/bash
# Runs linkpeer and tempohut side by side for a few seconds, changes tempo on
# one side, and prints the last JSON lines from both. Usage: interop_check.sh [tempo_side]
set -u
REPO=/usr/local/google/home/jophba/.gemini/jetski/scratch/tempo-sync-rs
OUT=/tmp/tempo-sync-interop
mkdir -p "$OUT"
cd "$REPO" || exit 1
cargo build --offline -q -p tempohut 2>&1 | tail -3
LP=$REPO/tools/linkpeer/linkpeer
TH=$REPO/target/debug/tempohut
[ -x "$LP" ] || { echo "linkpeer missing"; exit 1; }
IFACE=$(ip -4 -o addr show scope global | awk '{print $4}' | cut -d/ -f1 | head -1)
echo "iface=$IFACE"
mkfifo "$OUT/lp.in" "$OUT/th.in" 2>/dev/null
# keep fifos open for writing
exec 3<>"$OUT/lp.in" 4<>"$OUT/th.in"
"$LP" --tempo 128 --interval-ms 250 < "$OUT/lp.in" > "$OUT/lp.out" 2> "$OUT/lp.err" &
LPPID=$!
"$TH" --json --tempo 90 --interval-ms 250 ${IFACE:+--iface $IFACE} < "$OUT/th.in" > "$OUT/th.out" 2> "$OUT/th.err" &
THPID=$!
sleep 4
echo "--- after 4s"; tail -1 "$OUT/lp.out"; tail -1 "$OUT/th.out"
echo "tempo 100" >&3
sleep 3
echo "--- 3s after 'tempo 100' on linkpeer"; tail -1 "$OUT/lp.out"; tail -1 "$OUT/th.out"
echo "tempo 75" >&4
sleep 3
echo "--- 3s after 'tempo 75' on tempohut"; tail -1 "$OUT/lp.out"; tail -1 "$OUT/th.out"
echo "quit" >&3; echo "quit" >&4
sleep 1
kill $LPPID $THPID 2>/dev/null
echo "--- stderr tails"; tail -3 "$OUT/lp.err"; tail -3 "$OUT/th.err"
