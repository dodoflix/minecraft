#!/usr/bin/env bash
# Differential world-generation harness: generate the same area with the real vanilla server and with OUR server, then
# compare the two worlds' region files chunk by chunk with `worldcmp` (tools/worldcmp).
#
#   scripts/worldgen-diff.sh SEED DIM CENTER_X CENTER_Z RADIUS_CHUNKS [options]     (DIM = overworld|nether|end|all)
#   scripts/worldgen-diff.sh --seed S --dims D --center X,Z --radius R [--out DIR] [options]
#   scripts/worldgen-diff.sh --cases scripts/worldgen-diff-cases.txt [--only REGEX] [options]   (one run per case line)
#
#   --cases FILE             run every `seed dim center_x center_z radius [label]` line (# comments); --only REGEX filters
#                            lines; the exit status is the worst of the runs. Each run goes to run/wgdiff/<seed>-<dim>-<x>_<z>.
#   --seed S                 level-seed (default 12345)
#   --dims overworld,nether,end   dimensions to generate (default overworld)
#   --center X,Z             BLOCK coordinates of the area centre (default 0,0)
#   --radius R               radius in CHUNKS: the area is (2R+1)^2 chunks around the centre chunk (default 8)
#   --out DIR                output dir (default run/wgdiff/<seed>-<dims>-<cx>_<cz>); DIR/A/world, DIR/B/world, DIR/report.txt, DIR/report.json
#   --sides A,B              what runs as side A and B: vanilla|ours (default vanilla,ours). `vanilla,vanilla` is the
#                            determinism sanity run (harness vs vanilla's own nondeterminism).
#   --generator-settings S   server.properties generator-settings (flat presets etc.)
#   --level-type T           server.properties level-type (default minecraft:normal)
#   --difficulty D / --gamemode M   default normal / survival (difficulty changes which mobs spawn during worldgen)
#   --ours-bin PATH          our headless generator (default $WORLDGEN_AREA_BIN, else target/worldgen-diff/debug/examples/worldgen_area,
#                            rebuilt incrementally with `CARGO_TARGET_DIR=target/worldgen-diff cargo build -p mc-game --example worldgen_area`)
#   --order seq|batch        seq (default): one chunk at a time in raster order, each awaited; batch: all tiles at once (fast,
#                            but vanilla's cross-chunk feature writes then depend on thread scheduling: vanilla-vs-vanilla
#                            differs). Measured: seq + max.bg.threads=1, area away from the spawn => vanilla-vs-vanilla identical
#   --vanilla-jvm-opts "..." JVM flags for the vanilla server (default "-Dmax.bg.threads=1" = single worldgen worker)
#   --no-cache               do not reuse/store the cached vanilla world ($WGDIFF_CACHE, default ${XDG_CACHE_HOME:-$HOME/.cache}/mc-worldgen-diff)
#   --port N                 server port (default: a free one)
#   --timeout S              per-wait timeout in seconds (default 1800)
#   --compare-args "..."     extra args for worldcmp (it already gets --chunks <the area>: chunks around the area are full but
#                            only partly decorated and order dependent; a later --chunks in here wins), e.g. "--chunks -4,-4,4,4 --max-listed 30 --ignore-status-below features"
#   --no-compare             only generate
#
# How the area is generated
#   Vanilla (and ours, if it has the commands): for each dimension `execute in <dim> run forceload add x1 z1 x2 z2`
#   in tiles of at most 16x16 chunks (vanilla's 256-chunk command limit; tickets are cumulative, so the whole area is
#   force-loaded at once), then wait until every chunk of the area is FULL (`execute in <dim> if loaded <pos> ... run
#   say <tag>` re-sent every 2 s until the tag shows up in the log), then `save-all flush` and `stop`. `tick freeze` +
#   `random_tick_speed 0` are sent first so forced (ticking) chunks are not mutated by random/fluid/block ticks.
#   server.properties: online-mode=false, generate-structures=true, max-players=0, pause-when-empty-seconds=-1
#   (vanilla 26 otherwise stops ticking, and thus generating, 60 s after the last player left), view-distance=3.
#
#   Our server: `worldgen_area` (crates/mc-game/examples/worldgen_area.rs), which runs the REAL server (`create_server` +
#   `run_server`: initServer, tick loop, stopServer save) in the side dir with the same server.properties and queues one
#   step per tick: `tick freeze`, random_tick_speed 0, then per dimension in the same order and per chunk in raster order a
#   FORCED ticket (= `forceload add`), wait until the chunk is ENTITY_TICKING and the chunk map idle for 20 ticks (= the
#   `if loaded` + `sleep 1` above), finally halt => every level is saved like `save-all flush` + `stop`. Chunk generation
#   runs on the chunk map's one serial worldgen thread, like vanilla with -Dmax.bg.threads=1. `--order batch` only affects
#   vanilla. Vanilla worlds are cached under $WGDIFF_CACHE (default ${XDG_CACHE_HOME:-$HOME/.cache}/mc-worldgen-diff, shared by all checkouts)/<key> (key = every input that changes the vanilla world), so
#   reruns only regenerate ours; --no-cache forces regeneration.
#
# Results & comparison
#   NOTE the spawn area (chunks around the world spawn, ~(0,0)) is generated and ticks BEFORE `tick freeze` can be sent, so
#   its chunks are nondeterministic even vanilla-vs-vanilla (random ticks: vines, grass, ...): keep areas away from spawn
#   (distance > ~4 chunks) or ignore `blocks`/`ticks` there. Entity fields from the entity's own RNG are normalized by worldcmp.
#   Exit status: 0 iff worldcmp reports the worlds identical in every dimension, 1 if they differ, 2 on harness error.
#   worldcmp compares only chunks that are `minecraft:full` in BOTH worlds (see tools/worldcmp/src/main.rs) and ignores
#   LastUpdate, InhabitedTime, light data and entity UUIDs. Spawn-area chunks and the one-chunk ring around the area
#   (generated as non-full neighbours) are not special-cased: they are compared iff full on both sides.
#
# Examples
#   scripts/worldgen-diff.sh --seed 12345 --dims overworld --center 0,0 --radius 8 --out run/wgdiff/s12345
#   scripts/worldgen-diff.sh --seed 12345 --radius 4 --sides vanilla,vanilla --out run/wgdiff/sanity   # must be IDENTICAL
#   scripts/worldgen-diff.sh --seed 1 --dims nether,end --center 0,0 --radius 6 --out run/wgdiff/nether-end
#   (curated interesting places: scripts/worldgen-diff-areas.txt, `seed dim structure x z`)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
die() { echo "worldgen-diff: $*" >&2; exit 2; }
log() { echo "[worldgen-diff] $*" >&2; }

ORIG_ARGS=("$@")
# -XX:hashCode=2: every identity hash is 1, which makes vanilla's ReferenceOpenHashSet<ChunkHolder> iteration (the order in which
# neighbouring chunks are decorated) deterministic and reproducible by us (ConstHashReferenceSet); see worldgen-diff docs.
export VANILLA_JVM_OPTS="${VANILLA_JVM_OPTS--Dmax.bg.threads=1 -XX:+UnlockExperimentalVMOptions -XX:hashCode=2}"
SEED=12345 DIMS=overworld CENTER=0,0 RADIUS=8 OUT="" CASES="" ONLY="" SIDES=vanilla,ours GENSET="" LEVELTYPE=minecraft:normal
ORDER=seq DIFFICULTY=normal GAMEMODE=survival BIN="${WORLDGEN_AREA_BIN:-}" CACHE=1 PORT="" TMO=1800 CMPARGS="" COMPARE=1
if (($# >= 5)) && [[ "$1" != --* ]]; then SEED="$1" DIMS="$2" CENTER="$3,$4" RADIUS="$5"; shift 5; fi
while (($#)); do
  case "$1" in
    --cases) CASES="$2"; shift 2;;
    --only) ONLY="$2"; shift 2;;
    --seed) SEED="$2"; shift 2;;
    --dims) DIMS="$2"; shift 2;;
    --center) CENTER="$2"; shift 2;;
    --radius) RADIUS="$2"; shift 2;;
    --out) OUT="$2"; shift 2;;
    --sides) SIDES="$2"; shift 2;;
    --generator-settings) GENSET="$2"; shift 2;;
    --level-type) LEVELTYPE="$2"; shift 2;;
    --difficulty) DIFFICULTY="$2"; shift 2;;
    --gamemode) GAMEMODE="$2"; shift 2;;
    --ours-bin) BIN="$2"; shift 2;;
    --order) ORDER="$2"; shift 2;;
    --vanilla-jvm-opts) export VANILLA_JVM_OPTS="$2"; shift 2;;
    --no-cache) CACHE=0; shift;;
    --port) PORT="$2"; shift 2;;
    --timeout) TMO="$2"; shift 2;;
    --compare-args) CMPARGS="$2"; shift 2;;
    --no-compare) COMPARE=0; shift;;
    *) die "unknown option $1";;
  esac
done
if [[ -n "$CASES" ]]; then   # re-invoke ourselves once per case; extra options (sides, bin, compare-args, ...) are passed on
  passthru=()
  for ((i = 1; i <= ${#ORIG_ARGS[@]}; i++)); do
    case "${ORIG_ARGS[i-1]}" in --cases|--only) i=$((i + 1));; *) passthru+=("${ORIG_ARGS[i-1]}");; esac
  done
  rc=0
  while read -r seed dim x z radius label; do
    [[ -z "$seed" || "$seed" == \#* ]] && continue
    [[ -z "$ONLY" || "$seed $dim $x $z $radius $label" =~ $ONLY ]] || continue
    "$ROOT/scripts/worldgen-diff.sh" "$seed" "$dim" "$x" "$z" "$radius" "${passthru[@]}" || { r=$?; ((r > rc)) && rc=$r; }
  done < "$CASES"
  exit "$rc"
fi
[[ "$DIMS" == all ]] && DIMS=overworld,nether,end
[[ "$CENTER" =~ ^-?[0-9]+,-?[0-9]+$ ]] || die "--center must be X,Z (blocks)"
[[ "$RADIUS" =~ ^[0-9]+$ ]] || die "--radius must be a non-negative integer"
IFS=, read -r SIDE_A SIDE_B <<<"$SIDES"
for s in "$SIDE_A" "$SIDE_B"; do [[ "$s" == vanilla || "$s" == ours ]] || die "--sides must be vanilla|ours,vanilla|ours"; done
[[ -n "$PORT" ]] || PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
CX=$(( ${CENTER%,*} >> 4 )) CZ=$(( ${CENTER#*,} >> 4 ))   # arithmetic shift = floorDiv(x, 16)
[[ -n "$OUT" ]] || OUT="$ROOT/run/wgdiff/$SEED-${DIMS//,/+}-${CX}_$CZ"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
X1=$((CX - RADIUS)) X2=$((CX + RADIUS)) Z1=$((CZ - RADIUS)) Z2=$((CZ + RADIUS))

dim_id() { case "$1" in overworld) echo minecraft:overworld;; nether|the_nether) echo minecraft:the_nether;; end|the_end) echo minecraft:the_end;; *:*) echo "$1";; *) die "unknown dimension $1";; esac; }

# --- server dir helpers -------------------------------------------------------------------------------------------
# shellcheck disable=SC2016  # the inner script expands in the child shell
send() { timeout 10 sh -c 'printf "%s\n" "$1" > "$2"' _ "$2" "$1/stdin.fifo"; }   # send DIR "command" (never blocks on a dead server)
logf() { echo "$1/logs/latest.log"; }
# wait_log DIR REGEX [TIMEOUT]: wait for an extended regex in the server log (fails early if the process died)
wait_log() {
  local d="$1" re="$2" t="${3:-$TMO}" pid; pid="$(cat "$1/server.pid" 2>/dev/null || true)"
  for _ in $(seq $((t * 2))); do
    grep -qE -- "$re" "$(logf "$d")" 2>/dev/null && return 0
    [[ -z "$pid" ]] || kill -0 "$pid" 2>/dev/null || { tail -20 "$d/server.out" >&2; die "server in $d died while waiting for /$re/"; }
    sleep 0.5
  done
  die "timeout (${t}s) waiting for /$re/ in $(logf "$d")"
}
write_props() {   # write_props DIR VIEW_DISTANCE MAX_PLAYERS
  cat > "$1/server.properties" <<EOF
server-port=$PORT
online-mode=false
enforce-secure-profile=false
white-list=false
spawn-protection=0
view-distance=$2
simulation-distance=$2
gamemode=$GAMEMODE
difficulty=$DIFFICULTY
level-type=$LEVELTYPE
level-seed=$SEED
generate-structures=true
generator-settings=$GENSET
max-players=$3
pause-when-empty-seconds=-1
motd=worldgen-diff
EOF
  echo "eula=true" > "$1/eula.txt"
}

# --- generation by commands ---------------------------------------------------------------------------------------
# tiles(): "x1 z1 x2 z2" chunk rectangles of at most 16x16 chunks covering the area
tiles() {
  local x z
  for ((z = Z1; z <= Z2; z += 16)); do for ((x = X1; x <= X2; x += 16)); do
    echo "$x $z $((x + 15 > X2 ? X2 : x + 15)) $((z + 15 > Z2 ? Z2 : z + 15))"
  done; done
}

wait_tag() {   # wait_tag DIR TAG COMMAND SLEEP: re-send COMMAND until TAG appears in the log
  local d="$1" tag="$2" cmd="$3" nap="$4"
  for _ in $(seq $((TMO * 2))); do
    send "$d" "$cmd"
    sleep "$nap"
    grep -qF "[Server] $tag" "$(logf "$d")" && return 0
    kill -0 "$(cat "$d/server.pid")" 2>/dev/null || die "server died while generating"
  done
  die "timeout generating ($cmd)"
}

gen_by_commands() {   # gen_by_commands DIR
  local d="$1" dim k=0 tag x1 z1 x2 z2 cx cz chain n
  # forced chunks are ENTITY_TICKING: without this random ticks (kelp/vines/grass growth), fluid and block ticks and mob
  # spawning mutate them for as long as the server runs (differs run to run). Frozen, chunk loading/generation continues.
  send "$d" "tick freeze"; send "$d" "gamerule random_tick_speed 0"
  for dd in ${DIMS//,/ }; do
    dim="$(dim_id "$dd")"
    if [[ "$ORDER" == seq ]]; then
      # One chunk at a time in raster order. `if loaded` needs ENTITY_TICKING, which only forced chunks reach, so this waits
      # for the forced chunk itself (its FULL dependency ring is generated by then) and lets the pipeline settle 1 s.
      for ((cz = Z1; cz <= Z2; cz++)); do for ((cx = X1; cx <= X2; cx++)); do
        k=$((k + 1)); tag="WGDIFF-$k-$$"
        send "$d" "execute in $dim run forceload add $((cx * 16)) $((cz * 16))"
        wait_tag "$d" "$tag" "execute in $dim if loaded $((cx * 16)) 64 $((cz * 16)) run say $tag" 0.3
        sleep 1
      done; done
      log "$dim: $(( (X2 - X1 + 1) * (Z2 - Z1 + 1) )) chunks forced one by one (sequential)"
      continue
    fi
    while read -r x1 z1 x2 z2; do   # forceload tickets are cumulative: the whole area is requested at once
      send "$d" "execute in $dim run forceload add $((x1 * 16)) $((z1 * 16)) $((x2 * 16 + 15)) $((z2 * 16 + 15))"
    done < <(tiles)
    # wait until every chunk is FULL: chains of at most 256 `if loaded` conditions
    chain="" n=0
    for ((cz = Z1; cz <= Z2; cz++)); do for ((cx = X1; cx <= X2; cx++)); do
      chain+=" if loaded $((cx * 16)) 64 $((cz * 16))"; n=$((n + 1))
      if ((n == 256 || (cx == X2 && cz == Z2))); then
        k=$((k + 1)); tag="WGDIFF-$k-$$"
        wait_tag "$d" "$tag" "execute in $dim$chain run say $tag" 2
        chain="" n=0
      fi
    done; done
    log "$dim: $(( (X2 - X1 + 1) * (Z2 - Z1 + 1) )) chunks FULL"
  done
}

run_side() {   # run_side LABEL KIND
  local label="$2" kind="$1" d="$OUT/$2"
  rm -rf "$d"; mkdir -p "$d"
  log "side $label: $kind"
  if [[ "$kind" == vanilla ]]; then
    local name="wgdiff-$$-$label" vs="$ROOT/scripts/vanilla-server.sh" sd="$ROOT/run/servers/wgdiff-$$-$label" cache
    cache="${WGDIFF_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/mc-worldgen-diff}/$SEED-${DIMS//,/+}-${X1}_${Z1}_${X2}_$Z2-$(printf '%s' "$LEVELTYPE|$GENSET|$DIFFICULTY|$GAMEMODE|$ORDER|$VANILLA_JVM_OPTS" | sha1sum | cut -c1-8)"
    if [[ "$CACHE" == 1 && -d "$cache/world" ]]; then log "side $label: vanilla world from cache $cache"; cp -a "$cache/world" "$cache/server.properties" "$cache/logs" "$d/"; return; fi
    "$vs" start "$name" --port "$PORT" --seed "$SEED" --level-type "$LEVELTYPE" --view-distance 3 --simulation-distance 3 \
      --gamemode "$GAMEMODE" --difficulty "$DIFFICULTY" --reset -- generate-structures=true max-players=0 \
      pause-when-empty-seconds=-1 "generator-settings=$GENSET" </dev/null >"$d/start.out" 2>&1 || { cat "$d/start.out" >&2; die "vanilla start failed"; }
    gen_by_commands "$sd"
    send "$sd" "save-all flush"; wait_log "$sd" 'Saved the game' 300
    "$vs" stop "$name" >/dev/null </dev/null 2>&1
    cp -a "$sd/world" "$d/world"; cp -a "$sd/server.properties" "$sd/logs" "$d/"; rm -rf "$sd"
    if [[ "$CACHE" == 1 ]]; then   # complete worlds only: built aside, then renamed
      rm -rf "$cache.tmp"; mkdir -p "$cache.tmp"; cp -a "$d/world" "$d/server.properties" "$d/logs" "$cache.tmp/"; rm -rf "$cache"; mv "$cache.tmp" "$cache"
    fi
  else
    if [[ -z "$BIN" ]]; then   # always: stays current (incremental)
      (cd "$ROOT" && CARGO_TARGET_DIR=target/worldgen-diff cargo build -q -p mc-game --example worldgen_area 2>"$d/cargo.log") || { tail -30 "$d/cargo.log" >&2; die "cannot build worldgen_area"; }
      BIN="$ROOT/target/worldgen-diff/debug/examples/worldgen_area"
    fi
    BIN="$(realpath "$BIN")"; [[ -x "$BIN" ]] || die "worldgen_area binary not found: $BIN"
    write_props "$d" 3 0
    local dims=""; for dd in ${DIMS//,/ }; do dims+="${dims:+,}$(dim_id "$dd")"; done
    (cd "$d" && timeout "$TMO" "$BIN" --dims "$dims" --area "$X1,$Z1,$X2,$Z2" </dev/null >server.out 2>&1) || { tail -30 "$d/server.out" >&2; die "our generator failed (see $d/server.out)"; }
    log "$dims: generated by $(basename "$BIN")"
  fi
}

# shellcheck disable=SC2329  # runs from the EXIT trap below
cleanup() {
  for p in "$ROOT/run/servers/wgdiff-$$-"*/server.pid "$OUT"/*/server.pid; do
    [[ -f "$p" ]] && kill -9 "$(cat "$p")" 2>/dev/null || true
  done
}
trap cleanup EXIT

# --- main -----------------------------------------------------------------------------------------------------------
log "seed=$SEED dims=$DIMS area=chunks [$X1,$Z1]..[$X2,$Z2] sides=$SIDE_A,$SIDE_B port=$PORT out=$OUT"
run_side "$SIDE_A" A
run_side "$SIDE_B" B
[[ "$COMPARE" == 1 ]] || { log "generated; skipped compare"; exit 0; }

[[ -n "${WORLDCMP:-}" ]] || (cd "$ROOT" && CARGO_TARGET_DIR=target/worldcmp cargo build -q -p worldcmp) || die "cannot build worldcmp"   # always: stays current
WORLDCMP="${WORLDCMP:-$ROOT/target/worldcmp/debug/worldcmp}"
set +e
# shellcheck disable=SC2086
"$WORLDCMP" "$OUT/A/world" "$OUT/B/world" --dim "$DIMS" --chunks "$X1,$Z1,$X2,$Z2" --report "$OUT/report.json" $CMPARGS | tee "$OUT/report.txt"
rc=${PIPESTATUS[0]}
set -e
log "report: $OUT/report.txt, $OUT/report.json (worldcmp exit $rc)"
exit "$rc"
