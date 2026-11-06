#!/usr/bin/env bash
# Regenerates the oracle files in this directory with the real vanilla server (offline mode).
# Starting the server accepts the Minecraft EULA in your name: needs MC_ACCEPT_EULA=1 (see scripts/vanilla-server.sh).
# Usage: MC_ACCEPT_EULA=1 crates/mc-game/tests/fixtures/dedicated/lists/generate.sh
# Output: crates/mc-game/tests/fixtures/dedicated/lists/{ops,whitelist,banned-players,banned-ips,usercache}.json
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
S="$ROOT/scripts/vanilla-server.sh"
DEST="$ROOT/crates/mc-game/tests/fixtures/dedicated/lists"
D="$ROOT/run/servers/gsc-lists"
FILES=(ops.json whitelist.json banned-players.json banned-ips.json usercache.json)
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$S" start gsc-lists --reset --port 25999 --view-distance 2 --simulation-distance 2 </dev/null >/dev/null 2>&1
for n in Alice Bob Carol Dave Eve Frank Grace Heidi Ivan Judy Mallory Niaj Olivia Peggy Rupert Sybil Trent Victor; do
  "$S" cmd gsc-lists "op $n"; "$S" cmd gsc-lists "whitelist add $n"
done
for c in "ban Alice griefing <b>&x'=y" "ban Dave" "ban Trent spam" "ban-ip 1.2.3.4 ip reason" "ban-ip 10.0.0.1" \
         "ban-ip 192.168.1.77 x" "deop Bob" "whitelist remove Eve"; do "$S" cmd gsc-lists "$c"; done
sleep 4; "$S" stop gsc-lists
for f in "${FILES[@]}"; do cp "$D/$f" "$TMP/$f"; done
for f in "${FILES[@]}"; do mv "$TMP/$f" "$DEST/$f"; done
