#!/bin/bash
set -u
REPO=/usr/local/google/home/jophba/.gemini/jetski/scratch/tempo-sync-rs
OUT=/tmp/tempo-sync-interop-test
mkdir -p "$OUT"
cd "$REPO" || exit 1
LP=$REPO/tools/linkpeer/linkpeer
TH=$REPO/target/debug/tempohut
IFACE=$(ip -4 -o addr show scope global | awk '{print $4}' | cut -d/ -f1 | head -1)

run_test() {
    NODE_ID=$1
    TH_FIRST=$2
    echo "=== RUNNING TEST: NodeID=$NODE_ID TH_FIRST=$TH_FIRST ==="
    pkill -9 linkpeer 2>/dev/null || true
    pkill -9 tempohut 2>/dev/null || true
    rm -f "$OUT/lp.in" "$OUT/th.in"
    mkfifo "$OUT/lp.in" "$OUT/th.in" 2>/dev/null || true
    exec 3<>"$OUT/lp.in" 4<>"$OUT/th.in"
    
    if [ "$TH_FIRST" = "true" ]; then
        "$TH" --json --tempo 90 --interval-ms 250 --node-id-hex $NODE_ID ${IFACE:+--iface $IFACE} < "$OUT/th.in" > "$OUT/th.out" 2> "$OUT/th.err" &
        THPID=$!
        sleep 1
        "$LP" --tempo 128 --interval-ms 250 < "$OUT/lp.in" > "$OUT/lp.out" 2> "$OUT/lp.err" &
        LPPID=$!
    else
        "$LP" --tempo 128 --interval-ms 250 < "$OUT/lp.in" > "$OUT/lp.out" 2> "$OUT/lp.err" &
        LPPID=$!
        sleep 1
        "$TH" --json --tempo 90 --interval-ms 250 --node-id-hex $NODE_ID ${IFACE:+--iface $IFACE} < "$OUT/th.in" > "$OUT/th.out" 2> "$OUT/th.err" &
        THPID=$!
    fi
    
    sleep 3
    echo "--- after 3s (should be synced)"; tail -1 "$OUT/lp.out" | grep -o '"tempo": [^,]*'; tail -1 "$OUT/th.out" | grep -o '"tempo": [^,]*'
    
    echo "quit" >&3; echo "quit" >&4
    sleep 1
    kill -9 $LPPID $THPID 2>/dev/null || true
}

run_test "0000000000000000" "true"
run_test "0000000000000000" "false"
run_test "FFFFFFFFFFFFFFFF" "true"
run_test "FFFFFFFFFFFFFFFF" "false"
