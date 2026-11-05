#!/usr/bin/env bash
# Run ONE bot scenario (tests/e2e/mobs/*.scn, commands: crates/mc-testbot/src/scenario.rs) against the real vanilla server and against OUR
# server, one after the other (never both JVM/server processes at once), and diff what the client saw.
#
#   scripts/mob-e2e.sh SCENARIO.scn [--sides vanilla,ours] [--out DIR] [--seed N] [--ignore PACKET]... [-- mc-testbot diff args]
#
# Output (DIR default run/mob-e2e/NAME): vanilla.jsonl / ours.jsonl (packet logs), vanilla.obs / ours.obs (`observe` lines), diff.txt
# (`mc-testbot diff vanilla.jsonl ours.jsonl`) and obs.diff (plain diff of the observation files). Exit 0 iff both diffs are empty
# (with --sides vanilla only: just records the oracle). Needs: ref/server.jar, java, target/debug/{mc-testbot,mc-server}
# (`cargo build -p mc-testbot -p mc-server`; MC_SERVER_BIN / MC_TESTBOT_BIN override).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[ $# -ge 1 ] || { sed -n 2,12p "$0" >&2; exit 2; }
SCN="$(realpath "$1")"; shift
NAME="$(basename "$SCN" .scn)"
SIDES=vanilla,ours; OUT="$ROOT/run/mob-e2e/$NAME"; SEED=12345; DIFFARGS=()
while (($#)); do
  case "$1" in
    --sides) SIDES="$2"; shift 2;;
    --out) OUT="$(realpath -m "$2")"; shift 2;;
    --seed) SEED="$2"; shift 2;;
    --ignore) DIFFARGS+=(--ignore "$2"); shift 2;;
    --) shift; DIFFARGS+=("$@"); break;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
BOT="${MC_TESTBOT_BIN:-$ROOT/target/debug/mc-testbot}"
SERVER="${MC_SERVER_BIN:-$ROOT/target/debug/mc-server}"
[ -x "$BOT" ] || { echo "no mc-testbot binary: cargo build -p mc-testbot" >&2; exit 1; }
mkdir -p "$OUT"
PORT=$((20000 + RANDOM % 20000))
# same world for both sides: flat, normal difficulty, nothing spawns on its own, survival
PROPS=(level-type=minecraft:flat difficulty=normal spawn-monsters=false spawn-animals=false spawn-npcs=false allow-nether=false
       gamemode=survival force-gamemode=true generate-structures=false enforce-secure-profile=false spawn-protection=0)

run_bot() { # side
  "$BOT" run "127.0.0.1:$PORT" "$SCN" --name Bot --log "$OUT/$1.jsonl" --obs "$OUT/$1.obs"
}

run_vanilla() {
  local n="mob-e2e-$NAME"
  "$ROOT/scripts/vanilla-server.sh" start "$n" --port "$PORT" --seed "$SEED" --level-type minecraft:flat --gamemode survival --difficulty normal \
    --view-distance 4 --simulation-distance 4 --xmx 1G --reset "${PROPS[@]}" >/dev/null
  trap '"$ROOT/scripts/vanilla-server.sh" stop "$n" >/dev/null 2>&1 || true' RETURN
  "$ROOT/scripts/vanilla-server.sh" cmd "$n" "op Bot" >/dev/null   # the bot joins after this: `op` of an offline name works before the first login
  run_bot vanilla
}

run_ours() {
  [ -x "$SERVER" ] || { echo "no mc-server binary: cargo build -p mc-server" >&2; return 1; }
  local d; d="$(mktemp -d)"
  printf 'eula=true\n' > "$d/eula.txt"
  { printf 'online-mode=false\nserver-ip=127.0.0.1\nserver-port=%s\nlevel-seed=%s\nmax-tick-time=-1\nview-distance=4\nsimulation-distance=4\n' "$PORT" "$SEED"
    printf '%s\n' "${PROPS[@]}"; } > "$d/server.properties"
  mkfifo "$d/in"
  (cd "$d" && exec "$SERVER" nogui < <(tail -f "$d/in") > "$d/out.log" 2>&1) &
  local pid=$!
  trap 'echo stop > "$d/in"; sleep 3; kill "$pid" 2>/dev/null || true; pkill -P "$pid" tail 2>/dev/null || true; rm -rf "$d"' RETURN
  for _ in $(seq 1 240); do grep -q 'Done (' "$d/out.log" 2>/dev/null && break; sleep 1; done
  grep -q 'Done (' "$d/out.log" || { echo "our server did not start:"; tail -20 "$d/out.log"; return 1; }
  echo "op Bot" > "$d/in"; sleep 1
  run_bot ours
}

case ",$SIDES," in *,vanilla,*) run_vanilla;; esac
case ",$SIDES," in *,ours,*) run_ours;; esac

if [ -s "$OUT/vanilla.jsonl" ] && [ -s "$OUT/ours.jsonl" ]; then
  rc=0
  "$BOT" diff "$OUT/vanilla.jsonl" "$OUT/ours.jsonl" --state play "${DIFFARGS[@]}" > "$OUT/diff.txt" || rc=1
  diff -u "$OUT/vanilla.obs" "$OUT/ours.obs" > "$OUT/obs.diff" || rc=1
  echo "packet diff: $OUT/diff.txt  observation diff: $OUT/obs.diff  (exit $rc)"
  exit $rc
fi
echo "recorded: $OUT"
