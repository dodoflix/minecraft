#!/usr/bin/env bash
# Dual run of the headless game test server: the real `net.minecraft.gametest.Main` (ref/server.jar with
# -DbundlerMainClass) and our `mc-gametest`, on the same command lines; compares the exit code, the normalised console log,
# the layout of `<universe>/gametestworld` and the `--report` XML.
#
#   scripts/gametest-diff.sh [--record DIR | --golden DIR] [--ours-bin PATH] [--no-build] [--keep] [CASE...]
#   cases: pass abbrev ambiguous double_dash dup_tests lone_dash repeat3 repeat_bad single_dash verify verify_bad packs
#   missing verify_alone help (default: every case but verify, which runs 400 tests)
#
# --record DIR  vanilla side only; writes DIR/<case>.vanilla.txt (the committed recordings live in
#               crates/mc-server/tests/fixtures/gametest_main; record twice and diff before committing)
# --golden DIR  our side only, compared with DIR/<case>.vanilla.txt
# --keep        keeps the scratch directories (run/gametest-diff/<id>)
# Exit status: 0 identical, 1 differences, 2 harness error.
#
# NORMALISATION (only what vanilla does non-deterministically; two recordings of every case differ in exactly these, except
# the stack frames, which are a JVM artifact of vanilla's uncaught exceptions that this port does not print)
#   - the `[HH:MM:SS] [thread/LEVEL]: ` log prefix (a non-INFO level is kept), `Worker-Main-<n>` thread numbers (the advancement
#     load logs from a worker thread there), the bundler's `Unpacking` lines and the JVM's own warnings, "Can't keep up", the
#     `at ...` and `... N more` frame lines of an uncaught exception
#   - the random start position (`at position <POS>!`, `BlockPos{<POS>}` and `(x, y, z)` in test logs and failure messages),
#     elapsed times, the working directory
#   - the periodic progress bars (one is logged per tick while the game time stands still): only the last one before the
#     summary stays, its symbols sorted (the tests run in a random order)
#   - the order of the test batches (with the test logs inside them), of the names after `Starting re-run of tests:` and
#     of the XML `testcase` elements
#   - the order of the `Saving chunks for level` lines (vanilla's dimension order differs between runs)
#   - the region and poi files of the far test area: `r.<far>.<far>.mca` (the test position is random), and the saved
#     data `wandering_trader.dat` and `level.dat_old` (they exist only when the run lasted 1200 resp. 6000 ticks)
#   - the XML report's `time` and `timestamp`
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
need python3
BUILD=1 BIN="" KEEP=0 MODE=both DIR=""
CASES=()
while (($#)); do
  case "$1" in
    --no-build) BUILD=0; shift;;
    --ours-bin) BIN="$2"; shift 2;;
    --keep) KEEP=1; shift;;
    --record) MODE=record DIR="$2"; shift 2;;
    --golden) MODE=golden DIR="$2"; shift 2;;
    -*) die "unknown option $1";;
    *) CASES+=("$1"); shift;;
  esac
done
((${#CASES[@]})) || CASES=(pass abbrev ambiguous double_dash dup_tests lone_dash repeat3 repeat_bad single_dash verify_bad packs missing verify_alone help)
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
if [ "$MODE" != record ]; then
  if ((BUILD)); then
    (cd "$ROOT" && cargo build --locked -q -p mc-server) >&2 || { echo "gametest-diff: build failed" >&2; exit 2; }
  fi
  [ -n "$BIN" ] || BIN="$TARGET/debug/mc-gametest"
  [ -x "$BIN" ] || die "no binary $BIN"
fi
WORK="$ROOT/run/gametest-diff/$$-$(date +%H%M%S)"
mkdir -p "$WORK/vanilla" "$WORK/ours" "$WORK/packs-gt"
if ((KEEP)); then echo "gametest-diff: scratch directory $WORK" >&2; else trap 'rm -rf "$WORK"' EXIT; fi
cp -r "$ROOT/crates/mc-game/tests/fixtures/commands_corpus/gametest/gtpack" "$WORK/packs-gt/gtpack"

# The command line of a case (--universe and --report are added per side).
case_args() {
  case "$1" in
    pass) echo "--tests minecraft:always_pass" ;;
    abbrev) echo "--tes minecraft:always_pass --repeatC 2" ;;
    ambiguous) echo "--tests minecraft:always_pass --re 3" ;;
    double_dash) echo "--tests minecraft:always_pass -- --repeatCount 3" ;;
    dup_tests) echo "--tests minecraft:always_pass --tests nobody:nothing" ;;
    lone_dash) echo "- --tests minecraft:always_pass" ;;
    single_dash) echo "-tests minecraft:always_pass" ;;
    repeat3) echo "--tests minecraft:always_pass --repeatCount 3" ;;
    repeat_bad) echo "--tests minecraft:always_pass --repeatCount x" ;;
    verify) echo "--verify true --tests minecraft:always_pass" ;;
    verify_bad) echo "--verify true --repeatCount x" ;;
    packs) echo "--packs $WORK/packs-gt --tests gt:*" ;;
    missing) echo "--tests nobody:nothing" ;;
    verify_alone) echo "--verify true" ;;
    help) echo "--help" ;;
    *) die "unknown case $1" ;;
  esac
}

normalise_log() {
  sed -E \
    -e '/^(Unpacking |Starting net\.minecraft\.gametest\.Main|WARNING: )/d' \
    -e '/^\[[0-9:]{8}\] \[ServerMain\/INFO\]: \[STDERR\]: \t(at |\.\.\. [0-9]+ more)/d' \
    -e 's/^\[[0-9:]{8}\] \[([^]\/]*)\/INFO\]: /[\1] /' \
    -e 's/^\[[0-9:]{8}\] \[([^]\/]*)\/([A-Z]+)\]: /\2: [\1] /' \
    -e "/^WARN: \[Server thread\] Can't keep up!/d" \
    -e 's/Worker-Main-[0-9]+/Worker-Main-N/' \
    -e 's/^\[ServerMain\] (Loaded [0-9]+ advancements)/[Worker-Main-N] \1/' \
    -e 's/(at position )-?[0-9]+, 4, -?[0-9]+!/\1<POS>!/' \
    -e 's/BlockPos\{x=-?[0-9]+, y=-?[0-9]+, z=-?[0-9]+\}/BlockPos{<POS>}/g' \
    -e 's/(COMPLETE IN ).* =+/\1<TIME> ======/' \
    -e 's/(Time elapsed: )[0-9]+ ms/\1<MS> ms/' \
    -e "s/(cwd ')[^']*'/\1<CWD>'/" \
    -e 's/[[:space:]]+$//' |
    awk '
      function isort(a, m,  i, j, t) {
        for (i = 2; i <= m; i++) { t = a[i]; for (j = i - 1; j >= 1 && a[j] > t; j--) a[j + 1] = a[j]; a[j + 1] = t }
      }
      function flush(  i) {
        isort(sv, n)
        for (i = 1; i <= n; i++) print sv[i]
        n = 0
      }
      function flushblocks(  i, j, t, tb) {
        for (i = 2; i <= nb; i++) {
          t = bh[i]; tb = bt[i]
          for (j = i - 1; j >= 1 && bh[j] > t; j--) { bh[j + 1] = bh[j]; bt[j + 1] = bt[j] }
          bh[j + 1] = t; bt[j + 1] = tb
        }
        for (i = 1; i <= nb; i++) printf "%s", bt[i]
        nb = 0
      }
      /^\[Server thread\] \[[ +_xX]*\]$/ {
        m = split(substr($0, 18, length($0) - 18), ch, "")
        isort(ch, m)
        bar = "[Server thread] ["
        for (i = 1; i <= m; i++) bar = bar ch[i]
        bar = bar "]"
        next
      }
      /^\[Server thread\] =+ [0-9]+ GAME TESTS COMPLETE/ { flushblocks(); if (bar != "") print bar; bar = "" }
      /^\[Server thread\] Starting re-run of tests: / {
        flushblocks()
        m = split(substr($0, index($0, ": ") + 2), nm, ", ")
        isort(nm, m)
        out = "[Server thread] Starting re-run of tests: "
        for (i = 1; i <= m; i++) out = out (i > 1 ? ", " : "") nm[i]
        print out
        next
      }
      /^\[Server thread\] Running test environment / { bh[++nb] = $0; bt[nb] = $0 "\n"; next }
      nb > 0 { bt[nb] = bt[nb] $0 "\n"; next }
      /^\[Server thread\] Saving chunks for level / { sv[++n] = $0; next }
      { if (n) flush(); print }
      END { flushblocks(); if (n) flush() }'
}

# The XML report with one `testcase` per line in sorted order; `time`, `timestamp` and the random position are masked.
normalise_report() {
  sed -E \
    -e 's/ time="[^"]*"/ time="<TIME>"/g' \
    -e 's/ timestamp="[^"]*"/ timestamp="<TIME>"/' \
    -e 's/message="\(-?[0-9]+, -?[0-9]+, -?[0-9]+\) /message="(<POS>) /g' \
    -e 's/<testcase /\n&/g' \
    -e 's/<\/testsuite>/\n&/g' |
    awk '
      function flush(  i, j, t) {
        for (i = 2; i <= n; i++) { t = tc[i]; for (j = i - 1; j >= 1 && tc[j] > t; j--) tc[j + 1] = tc[j]; tc[j + 1] = t }
        for (i = 1; i <= n; i++) print tc[i]
        n = 0
      }
      /^<testcase / { tc[++n] = $0; next }
      { if (n) flush(); print }
      END { if (n) flush() }'
}

normalise_layout() {
  (cd "$1" && find . -mindepth 1 | sort) |
    awk '/\/data\/minecraft\/wandering_trader\.dat$|\/level\.dat_old$/ { next }
    {
      if (match($0, /r\.-?[0-9]+\.-?[0-9]+\.mca$/)) {
        n = split(substr($0, RSTART + 2, RLENGTH - 6), c, ".")
        x = c[1] + 0; z = c[2] + 0
        if (x < -1 || x > 0 || z < -1 || z > 0) sub(/r\.-?[0-9]+\.-?[0-9]+\.mca$/, "r.<far>.<far>.mca")
      }
      print
    }' | sort -u
}

# Every value of a gzip NBT file as sorted `path = value` lines (arrays by their length); the game time and the save
# time differ between runs.
dump_nbt() {
  python3 - "$1" <<'PY' | sed -E 's/^(\.Data\.(Time|LastPlayed)) = .*/\1 = <N>/'
import gzip, struct, sys

SIZES = {7: 1, 11: 4, 12: 8}
SCALARS = {1: ("b", ">b", 1), 2: ("s", ">h", 2), 3: ("", ">i", 4), 4: ("L", ">q", 8), 5: ("f", ">f", 4), 6: ("d", ">d", 8)}


def read(buf, pos, tag, path, out):
    if tag in SCALARS:
        suffix, fmt, size = SCALARS[tag]
        out.append(f"{path} = {struct.unpack_from(fmt, buf, pos)[0]}{suffix}")
        return pos + size
    if tag in SIZES:
        n = struct.unpack_from(">i", buf, pos)[0]
        out.append(f"{path} = <array {n}>")
        return pos + 4 + n * SIZES[tag]
    if tag == 8:
        n = struct.unpack_from(">H", buf, pos)[0]
        out.append(f"{path} = {buf[pos + 2:pos + 2 + n].decode('utf8', 'replace')!r}")
        return pos + 2 + n
    if tag == 9:
        element, n = buf[pos], struct.unpack_from(">i", buf, pos + 1)[0]
        pos += 5
        out.append(f"{path} = <list {n}>")
        for i in range(n):
            pos = read(buf, pos, element, f"{path}[{i}]", out)
        return pos
    while buf[pos] != 0:
        n = struct.unpack_from(">H", buf, pos + 1)[0]
        name = buf[pos + 3:pos + 3 + n].decode("utf8")
        pos = read(buf, pos + 3 + n, buf[pos], f"{path}.{name}", out)
    return pos + 1


buf = gzip.open(sys.argv[1]).read()
out = []
read(buf, 3 + struct.unpack_from(">H", buf, 1)[0], 10, "", out)
print("\n".join(sorted(out)))
PY
}

# run_side SIDE CASE: writes $WORK/CASE.SIDE.txt
run_side() {
  local side=$1 case=$2 dir="$WORK/$1" args code=0
  # shellcheck disable=SC2046
  set -- $(case_args "$case")
  args=("$@")
  [ "$case" = help ] || args+=(--universe "u-$case" --report "rep-$case.xml")
  (
    cd "$dir"
    if [ "$side" = vanilla ]; then
      timeout 1800 java -DbundlerMainClass=net.minecraft.gametest.Main -jar "$ROOT/ref/server.jar" "${args[@]}" >"out-$case.txt" 2>"err-$case.txt" || echo $? >"code-$case.txt"
    else
      timeout 1800 "$BIN" "${args[@]}" >"out-$case.txt" 2>"err-$case.txt" || echo $? >"code-$case.txt"
    fi
  )
  [ -f "$dir/code-$case.txt" ] && code=$(cat "$dir/code-$case.txt")
  {
    echo "== exit"
    echo "$code"
    echo "== log"
    normalise_log <"$dir/out-$case.txt"
    if [ "$case" = help ]; then
      echo "== help"
      grep -v '^WARNING: ' "$dir/err-$case.txt" || true
    fi
    if [ -d "$dir/u-$case/gametestworld" ]; then
      echo "== layout"
      normalise_layout "$dir/u-$case/gametestworld"
      if [ -f "$dir/u-$case/gametestworld/level.dat" ]; then
        echo "== level.dat"
        dump_nbt "$dir/u-$case/gametestworld/level.dat"
      fi
    fi
    if [ -f "$dir/rep-$case.xml" ]; then
      echo "== report"
      normalise_report <"$dir/rep-$case.xml"
    fi
  } >"$WORK/$case.$side.txt"
}

rc=0
for c in "${CASES[@]}"; do
  case "$MODE" in
    record)
      run_side vanilla "$c"
      mkdir -p "$DIR"
      cp "$WORK/$c.vanilla.txt" "$DIR/$c.vanilla.txt"
      echo "gametest-diff: $c: RECORDED"
      ;;
    golden)
      run_side ours "$c"
      if diff -u "$DIR/$c.vanilla.txt" "$WORK/$c.ours.txt" >"$WORK/$c.diff"; then
        echo "gametest-diff: $c: IDENTICAL"
      else
        cat "$WORK/$c.diff"
        echo "gametest-diff: $c: DIFFERENT"
        rc=1
      fi
      ;;
    *)
      run_side vanilla "$c"
      run_side ours "$c"
      if diff -u "$WORK/$c.vanilla.txt" "$WORK/$c.ours.txt" >"$WORK/$c.diff"; then
        echo "gametest-diff: $c: IDENTICAL"
      else
        cat "$WORK/$c.diff"
        echo "gametest-diff: $c: DIFFERENT"
        rc=1
      fi
      ;;
  esac
done
exit $rc
