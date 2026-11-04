#!/usr/bin/env bash
# Regenerates the oracle files in this directory with the real vanilla server (offline mode) from the repo root.
set -euo pipefail
S=scripts/vanilla-server.sh
$S start gsc-lists --reset --port 25999 --view-distance 2 --simulation-distance 2 </dev/null >/dev/null 2>&1
for n in Alice Bob Carol Dave Eve Frank Grace Heidi Ivan Judy Mallory Niaj Olivia Peggy Rupert Sybil Trent Victor; do
  $S cmd gsc-lists "op $n"; $S cmd gsc-lists "whitelist add $n"
done
for c in "ban Alice griefing <b>&x'=y" "ban Dave" "ban Trent spam" "ban-ip 1.2.3.4 ip reason" "ban-ip 10.0.0.1" \
         "ban-ip 192.168.1.77 x" "deop Bob" "whitelist remove Eve"; do $S cmd gsc-lists "$c"; done
sleep 4; $S stop gsc-lists
D=run/servers/gsc-lists
cp $D/ops.json $D/whitelist.json $D/banned-players.json $D/banned-ips.json $D/usercache.json crates/mc-game/tests/fixtures/dedicated/lists/
