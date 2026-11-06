#!/usr/bin/env bash
# Regenerates the stats golden from the real vanilla ServerStatsCounter.
# Usage:
#   scripts/golden-stats.sh
#
# Output: crates/mc-game/tests/fixtures/stats_golden.txt (replaced only when the generator succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
F="$ROOT/crates/mc-game/tests/fixtures"
# -XX:hashCode=2 gives every identity hash the same value: the key order of the stat types (HashMap over StatType objects) is then
# the same in every run
quiet env JAVA_TOOL_OPTIONS="-XX:+UnlockExperimentalVMOptions -XX:hashCode=2" "$ROOT/scripts/vanilla-java.sh" -o "$F/stats_golden.txt" "$F/StatsGolden.java" "$F/stats_golden.txt"
wc -l "$F/stats_golden.txt"
