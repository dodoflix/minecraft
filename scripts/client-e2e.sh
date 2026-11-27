#!/usr/bin/env bash
# Run one real-client scenario (tests/e2e/client/*.cscn, grammar: scripts/client_e2e/scenario.py) against the vanilla server and
# against OUR server, one after the other, and compare what the real 26.3 client saw: state snapshots, chat, screen sequence,
# disconnect texts, client and server logs, screenshots.
#
#   scripts/client-e2e.sh SCENARIO.cscn [--sides vanilla,ours] [--out DIR] [--seed N] [--display xvfb|gamescope]
#                         [--known FILE] [--allow FILE] [--server-allow FILE]
#
# --sides takes `vanilla` and `ours`, optionally with property overrides (`vanilla,vanilla:seed=99` compares vanilla with a
# vanilla server of another seed; a lone side only records and gates its logs). Output (DIR default run/client-e2e/NAME):
# per side a directory with the artifacts, report.json and diff.txt. Exit 0 iff every difference is listed in
# tests/e2e/client/known-differences.txt (`SCENARIO KEY` per line); entries that no longer occur are reported.
# Needs: ref/server.jar, java, the real client (scripts/real-client.sh) and target/debug/{mc-server,mock-mojang}
# (cargo build --locked -p mc-server -p mc-testkit --bin mock-mojang; MC_SERVER_BIN / MC_MOCK_MOJANG_BIN override).
# Cleanup: a side runs at most MC_E2E_SIDE_TIMEOUT seconds (1800); on that limit, on INT/TERM and on a normal end the runner
# stops the side's server, clients and mock. A runner that dies without cleanup (SIGKILL) has them stopped by a watcher
# process (scripts/client_e2e/reaper.py) that outlives it.
# MC_ACCEPT_EULA=1 is required (both servers).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need python3 java
[ $# -ge 1 ] || { usage >&2; exit 2; }
require_eula
# relative SCENARIO/--out/--known paths stay relative to the caller: the runner uses absolute paths for everything else
PYTHONPATH="$ROOT/scripts${PYTHONPATH:+:$PYTHONPATH}" exec python3 -m client_e2e.runner "$@"
