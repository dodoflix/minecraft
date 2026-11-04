#!/usr/bin/env bash
# Regenerates crates/mc-data/tests/fixtures/block_golden.json from the vanilla classes (needs ref/ and scripts/extract.sh's build).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
F="$ROOT/crates/mc-data/tests/fixtures"
cd "$ROOT"
java -cp "$ROOT/tools/extractor/build:$CP" "$F/GoldenBlocks.java" "$F/block_golden.json" >/dev/null 2>&1
ls -la "$F/block_golden.json"
