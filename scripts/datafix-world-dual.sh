#!/usr/bin/env bash
# World dual: the real server and mc-server open identical copies of worlds that older vanilla servers generated, then
# the two server directories (and the log lines) are compared. Also the legacy user list dual and the upgrade flags
# (--forceUpgrade, --eraseCache, --recreateRegionFiles).
#
#   Usage: scripts/datafix-world-dual.sh [--nightly | --all | ID...] [--normal-worlds] [--cases CASE[,CASE..]] [--keep DIR]
#
#   ID...            corpus worlds by version id (see scripts/datafix-worlds.sh --list)
#   --nightly        1.12.2, 1.16.5, 1.20.4 and 1.21.11 (the default)
#   --all            every version of crates/mc-datafix/tests/fixtures/versions.tsv
#   --normal-worlds  also the seed-12345 worlds (<id>-normal), not only the flat scripted ones
#   --cases LIST     normal, newer, legacy-users, gates, resume, upgrade (default: all of them; gates = the start-up
#                    checks on hand-made level.dat files, resume = a world with an interrupted file fix; neither takes
#                    the worlds; upgrade always runs on the 26.2 corpus worlds)
#   --keep DIR       keep both server directories of every run under DIR/<case>/<world>/{vanilla,ours}
#
# The cases are the tests of crates/mc-server/tests/oracle/{world_start,legacy_users,world_upgrade}.rs; what they compare
# and what they normalise is documented there. The upgrade case always runs on the 26.2 corpus worlds (26.2 and
# 26.2-normal), whatever IDs are selected. A failure prints the first differing log record and the differing files and chunks.
# Env: MC_ACCEPT_EULA=1 (running a server accepts the Minecraft EULA in your name), MC_DATAFIX_CACHE (default
#      ${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus; scripts/datafix-worlds.sh fills it), DATAFIX_DUAL_WORLDS,
#      DATAFIX_DUAL_KINDS and WORLD_UPGRADE_KEEP (what the options above set).
# Needs: java 25 or newer, ref/ (scripts/setup-ref.sh), the corpus worlds of the selected versions.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need cargo java
require_eula

worlds=1.12.2,1.16.5,1.20.4,1.21.11 kinds=flat cases="" ids=()
while [ $# -gt 0 ]; do
  case $1 in
    --nightly) worlds=1.12.2,1.16.5,1.20.4,1.21.11 ;;
    --all) worlds=all ;;
    --normal-worlds) kinds=flat,normal ;;
    --cases) [ $# -ge 2 ] || die "--cases needs a list"; cases=$2; shift ;;
    --keep) [ $# -ge 2 ] || die "--keep needs a directory"; WORLD_UPGRADE_KEEP=$(realpath -m "$2"); export WORLD_UPGRADE_KEEP; shift ;;
    -*) die "unknown option $1 (see --help)" ;;
    *) ids+=("$1") ;;
  esac
  shift
done
if [ ${#ids[@]} -gt 0 ]; then worlds=$(IFS=,; echo "${ids[*]}"); fi
export DATAFIX_DUAL_WORLDS=$worlds DATAFIX_DUAL_KINDS=$kinds

# test_for CASE: the test name filter of a case.
test_for() {
  case $1 in
    normal) echo world_start::normal_start ;;
    newer) echo world_start::world_newer_than_the_current_version ;;
    legacy-users) echo legacy_users:: ;;
    gates) echo startup_gates:: ;;
    resume) echo file_fix_resume:: ;;
    upgrade) echo world_upgrade:: ;;
    *) die "unknown case $1" ;;
  esac
}

filters=()
for c in $(echo "${cases:-normal,newer,legacy-users,gates,resume,upgrade}" | tr , ' '); do
  filters+=("$(test_for "$c")")
done
cd "$ROOT"
# the filters are substrings: run every case in one test binary so the cargo build happens once
exec cargo test -p mc-server --test oracle -- --ignored --nocapture --test-threads=1 "${filters[@]}"
