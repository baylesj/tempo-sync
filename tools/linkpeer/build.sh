#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_LINK_DIR="$SCRIPT_DIR/../../../tempo-sync-refs/link"

LINK_DIR="${LINK_DIR:-"$DEFAULT_LINK_DIR"}"

if [ ! -d "$LINK_DIR" ]; then
  echo "Error: Ableton Link directory not found at $LINK_DIR" >&2
  echo "Set LINK_DIR to the root of the Ableton Link repository." >&2
  exit 1
fi

LINK_DIR="$(cd "$LINK_DIR" && pwd)"

echo "Building linkpeer using Link from $LINK_DIR..."

g++ -std=c++17 -O2 \
  -I"${LINK_DIR}/include" \
  -I"${LINK_DIR}/modules/asio-standalone/asio/include" \
  -DLINK_PLATFORM_LINUX=1 \
  -Wno-multichar \
  -pthread \
  "${SCRIPT_DIR}/linkpeer.cpp" \
  -o "${SCRIPT_DIR}/linkpeer"

echo "Successfully built ${SCRIPT_DIR}/linkpeer"
