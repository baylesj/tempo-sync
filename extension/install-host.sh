#!/bin/sh
# Builds tempo-sync-host and registers it with Chrome and Chromium for this user, so the
# Web Tempo Sync extension can start it. Pass --uninstall to remove the registrations.
#
# macOS and Linux only. Windows registers native hosts in the registry instead; see
# https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging#native-messaging-host-location
set -eu

NAME=es.bayl.tempo_sync
# Fixed by the "key" in manifest.json, so it's the same for every unpacked install.
EXTENSION_ID=ddfdmjnclbpgiijdcbjbginpnblceeep

case "$(uname -s)" in
  Darwin) base="$HOME/Library/Application Support" ;;
  Linux) base="$HOME/.config" ;;
  *)
    echo "install-host.sh supports macOS and Linux only." >&2
    exit 1
    ;;
esac

# The browser profile directories have spaces in them on macOS, so split on newlines only.
IFS='
'
profiles() {
  case "$(uname -s)" in
    Darwin)
      printf '%s\n' "$base/Google/Chrome" "$base/Google/Chrome Beta" "$base/Google/Chrome Canary" "$base/Chromium"
      ;;
    Linux)
      printf '%s\n' "$base/google-chrome" "$base/google-chrome-beta" "$base/google-chrome-unstable" "$base/chromium"
      ;;
  esac
}

if [ "${1:-}" = "--uninstall" ]; then
  for dir in $(profiles); do
    rm -f "$dir/NativeMessagingHosts/$NAME.json"
  done
  echo "Removed the $NAME registrations."
  exit 0
fi

repo="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release --manifest-path "$repo/Cargo.toml" -p tempo-sync-host
binary="$repo/target/release/tempo-sync-host"

for dir in $(profiles); do
  # Only register with browsers that are installed (their profile directory exists).
  [ -d "$dir" ] || continue
  mkdir -p "$dir/NativeMessagingHosts"
  cat > "$dir/NativeMessagingHosts/$NAME.json" <<EOF
{
  "name": "$NAME",
  "description": "Web Tempo Sync helper: joins Link sessions for the extension",
  "path": "$binary",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXTENSION_ID/"]
}
EOF
  echo "Registered with $dir"
done
