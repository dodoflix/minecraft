#!/usr/bin/env bash
# Regenerates the menu, item, container-menu and beacon goldens from the vanilla classes.
# Usage:
#   scripts/golden-menu.sh
#
# Output: crates/mc-game/tests/fixtures/{menu,container_menu}_golden.txt.gz, {item,beacon}_golden.txt (each replaced only when its generator succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh) and the extractor build for Setup.bootstrap (scripts/extract.sh; MC_EXTRACTOR_BUILD overrides its directory).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
export MC_EXTRA_CP="${MC_EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
F="$ROOT/crates/mc-game/tests/fixtures"
VJ="$ROOT/scripts/vanilla-java.sh"
items() { quiet "$VJ" "$F/ItemGolden.java" | grep -E '^(tool|use) '; }
"$VJ" -o "$F/menu_golden.txt.gz" "$F/MenuGolden.java"
zcat "$F/menu_golden.txt.gz" | wc -l
to_file "$F/item_golden.txt" items
wc -l "$F/item_golden.txt"
"$VJ" -o "$F/container_menu_golden.txt.gz" "$F/ContainerMenuGolden.java"
zcat "$F/container_menu_golden.txt.gz" | wc -l
quiet "$VJ" -o "$F/beacon_golden.txt" "$F/BeaconGolden.java"
wc -l "$F/beacon_golden.txt"
