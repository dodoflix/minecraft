#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/stats_golden.txt from the real vanilla ServerStatsCounter.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
F="$ROOT/crates/mc-game/tests/fixtures"
java -cp "$("$ROOT/scripts/vanilla-cp.sh")" "$F/StatsGolden.java" > "$F/stats_golden.txt"
wc -l "$F/stats_golden.txt"
