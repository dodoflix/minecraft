#!/usr/bin/env bash
# Regenerates golden.txt.gz, src/java_cased_letters.rs and src/word_break_data.bin from the real JDK (CaseGolden.java,
# GenCasedLetters.java, the JDK's compiled word break rules `sun/text/resources/WordBreakIteratorData`).
# Usage: crates/mc-core/tests/fixtures/java_case/gen.sh
# Output: crates/mc-core/tests/fixtures/java_case/golden.txt.gz, crates/mc-core/src/java_cased_letters.rs,
#         crates/mc-core/src/word_break_data.bin
# Needs: java, javac, jimage (JDK 25: the version the vanilla server runs on; other versions know other Unicode characters).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac jimage
HERE="$ROOT/crates/mc-core/tests/fixtures/java_case"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -d "$OUT" "$HERE/CaseGolden.java" "$HERE/GenCasedLetters.java"
java -cp "$OUT" CaseGolden "$OUT/golden.txt"
gzip -9n <"$OUT/golden.txt" >"$OUT/golden.txt.gz"
mv "$OUT/golden.txt.gz" "$HERE/golden.txt.gz"
java -cp "$OUT" GenCasedLetters "$ROOT/crates/mc-core/src/java_cased_letters.rs"
JAVA_HOME_DIR="$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")"
jimage extract --include 'regex:.*/WordBreakIteratorData$' --dir "$OUT/image" "$JAVA_HOME_DIR/lib/modules"
cp "$OUT/image/java.base/sun/text/resources/WordBreakIteratorData" "$ROOT/crates/mc-core/src/word_break_data.bin"
wc -l <"$OUT/golden.txt"
