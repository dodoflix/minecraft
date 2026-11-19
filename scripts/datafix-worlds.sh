#!/usr/bin/env bash
# Old vanilla server jars and the scripted worlds they leave behind: the real-world input of the data fixer tests.
#
#   Usage: scripts/datafix-worlds.sh [--force] [ID...]       fetch the jars, generate the worlds, record hashes (no ID: every listed version)
#          scripts/datafix-worlds.sh --list                  the listed versions with era, Java major and jar state (no network)
#          scripts/datafix-worlds.sh --download [ID...]      only fetch and verify the jars and record their rows in versions.tsv
#          scripts/datafix-worlds.sh --hashes [ID...]        rewrite world_hashes/ for the worlds already in the cache
#
# Output: $MC_DATAFIX_CACHE/jars/<id>.jar (sha1 checked against the Mojang version manifest; never tracked),
#         $MC_DATAFIX_CACHE/worlds/<id>/ (flat world, scripted) and <id>-normal/ (seed 12345, structures), exactly as the
#         server left them, $MC_DATAFIX_CACHE/logs/<id>-<kind>.log (server log and the command lines that failed),
#         crates/mc-datafix/tests/fixtures/versions.tsv (id, DataVersion, jar sha1, Java major, jar_runs) and
#         crates/mc-datafix/tests/fixtures/world_hashes/<id>.txt (sha256 of every world file).
# Env: MC_DATAFIX_CACHE (default ${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus),
#      MC_ACCEPT_EULA=1 (running a server accepts the Minecraft EULA in your name; --list/--download/--hashes need no opt-in),
#      MC_OLD_JAVA=<JDK 8 home> (releases up to 1.16.5 need it; without it they are skipped with a message, no JDK is ever downloaded),
#      DATAFIX_CMD_DELAY (seconds between console commands, default 0.3).
# Era scripts: crates/mc-datafix/tests/corpus/eras/e1..e5.txt (flat) and eN-normal.txt, one console command per line.
#      `# text` comment, `@wait REGEX [SECONDS]` wait for a server log line, `@sleep SECONDS`, `@since V cmd` / `@until V cmd`
#      (V exclusive) run cmd only for ids at or above / below V, `@Y@` is the y of the surface of the flat world, `@Y1@`..`@Y4@` the blocks above it.
# Needs: curl jq unzip sha1sum sha256sum python3; a JDK matching each jar (any installed JDK of at least the required major).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need curl jq unzip sha1sum sha256sum python3

CACHE="${MC_DATAFIX_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus}"
FIXTURES="$ROOT/crates/mc-datafix/tests/fixtures"
ERAS="$ROOT/crates/mc-datafix/tests/corpus/eras"
TSV="$FIXTURES/versions.tsv"
TSV_HEADER=$'id\tdata_version\tjar_sha1\tjava_major\tjar_runs'
MANIFEST_URL="https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
SEED=12345
VS="$ROOT/scripts/vanilla-server.sh"
DELAY="${DATAFIX_CMD_DELAY:-0.3}"

# One version per era boundary.
LISTED=(1.7.10 1.8.9 1.9.4 1.10.2 1.11.2 1.12.2 1.13 1.13.2 1.14.4 1.15.2 1.16.5 1.17.1 1.18.2 1.19.4 1.20.1 1.20.4 1.20.6
  1.21.1 1.21.4 1.21.5 1.21.11 26.1 26.2)

# ver_ge A B: version A is at least B.
ver_ge() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$2" ]; }

# era_of ID: e1 = 1.7-1.8, e2 = 1.9-1.12, e3 = 1.13-1.15, e4 = 1.16-1.20.4, e5 = 1.20.5 and newer (components).
era_of() {
  local minor patch
  case $1 in
    26.*) echo e5; return ;;
    1.*) ;;
    *) die "$1: no era for this id (release ids 1.7 and newer, 26.x)" ;;
  esac
  IFS=. read -r _ minor patch <<<"${1%%-*}"
  patch="${patch:-0}"
  if [ "$minor" -lt 7 ]; then die "$1: the data fixers start at 1.7 (older worlds are McRegion)"
  elif [ "$minor" -le 8 ]; then echo e1
  elif [ "$minor" -le 12 ]; then echo e2
  elif [ "$minor" -le 15 ]; then echo e3
  elif [ "$minor" -le 19 ] || { [ "$minor" -eq 20 ] && [ "$patch" -le 4 ]; }; then echo e4
  else echo e5; fi
}

# ---- manifest, jars ---------------------------------------------------------------------------------------------

# version_json ID: path of the cached per-version json.
version_json() {
  local id=$1 f="$CACHE/json/$1.json" url
  if [ ! -s "$f" ]; then
    mkdir -p "$CACHE/json"
    [ -s "$CACHE/version_manifest_v2.json" ] ||
      { curl -fsSL --retry 3 -o "$CACHE/version_manifest_v2.json.part" "$MANIFEST_URL" && mv "$CACHE/version_manifest_v2.json.part" "$CACHE/version_manifest_v2.json"; }
    url="$(jq -r --arg v "$id" '.versions[] | select(.id == $v) | .url' "$CACHE/version_manifest_v2.json")"
    [ -n "$url" ] || die "$id is not in the Mojang version manifest"
    curl -fsSL --retry 3 -o "$f.part" "$url" && mv "$f.part" "$f"
  fi
  echo "$f"
}

# fetch_jar ID: downloads CACHE/jars/ID.jar when missing or its sha1 differs, verified against the version json.
fetch_jar() {
  local id=$1 json jar sha1 url
  json="$(version_json "$id")"
  jar="$CACHE/jars/$id.jar"
  sha1="$(jq -r '.downloads.server.sha1 // empty' "$json")"
  url="$(jq -r '.downloads.server.url // empty' "$json")"
  [ -n "$url" ] || die "$id has no server download"
  if [ -f "$jar" ] && [ "$(sha1sum <"$jar" | cut -d' ' -f1)" = "$sha1" ]; then return 0; fi
  mkdir -p "$CACHE/jars"
  echo "datafix-worlds: downloading $id server jar" >&2
  curl -fSL --retry 3 -o "$jar.part" "$url"
  echo "$sha1  $jar.part" | sha1sum -c --quiet - || { rm -f "$jar.part"; die "sha1 mismatch for $id"; }
  mv "$jar.part" "$jar"
}

# jar_data_version JAR: world_version of the version.json inside the jar (or inside the nested game jar of a bundler jar);
# empty when the jar has none (releases before 1.14 carry no DataVersion outside their worlds).
jar_data_version() {
  local jar=$1 v inner tmp
  v="$(unzip -p "$jar" version.json 2>/dev/null | jq -r '.world_version // empty' 2>/dev/null)" || true
  if [ -z "$v" ]; then
    inner="$(unzip -Z1 "$jar" 'META-INF/versions/*.jar' 2>/dev/null | head -n1)" || true
    if [ -n "$inner" ]; then
      tmp="$(mktemp)"
      unzip -p "$jar" "$inner" >"$tmp" 2>/dev/null || true
      v="$(unzip -p "$tmp" version.json 2>/dev/null | jq -r '.world_version // empty' 2>/dev/null)" || true
      rm -f "$tmp"
    fi
  fi
  echo "$v"
}

# level_data_version LEVEL_DAT: Data.DataVersion, -1 when absent (1.7 and 1.8 worlds; vanilla treats them as -1).
level_data_version() {
  python3 -c '
import importlib, sys
sys.path.insert(0, sys.argv[1])
d = importlib.import_module("level-dat-spawn").load(sys.argv[2])["Data"]
print(d.get("DataVersion", -1))' "$ROOT/scripts" "$1"
}

# ---- versions.tsv -----------------------------------------------------------------------------------------------

# tsv_get ID COLUMN: the field (1-based column) of ID's row, empty when there is none.
tsv_get() { [ -f "$TSV" ] && awk -F'\t' -v id="$1" -v c="$2" '$1 == id { print $c }' "$TSV" || true; }

# tsv_put ID DATA_VERSION JAR_SHA1 JAVA_MAJOR JAR_RUNS: replaces ID's row; rows stay in LISTED order, others by id.
tsv_put() {
  local tmp
  mkdir -p "$FIXTURES"
  tmp="$(mktemp)"
  {
    echo "$TSV_HEADER"
    { [ -f "$TSV" ] && awk -F'\t' -v id="$1" 'NR > 1 && $1 != id' "$TSV"; printf '%s\t%s\t%s\t%s\t%s\n' "$@"; } |
      awk -F'\t' -v order="${LISTED[*]}" 'BEGIN { n = split(order, a, " "); for (i = 1; i <= n; i++) r[a[i]] = i }
        { print (($1 in r) ? r[$1] : 999) "\t" $0 }' | sort -t$'\t' -k1,1n -k2,2V | cut -f2-
  } >"$tmp"
  chmod 644 "$tmp"
  mv "$tmp" "$TSV"
}

# record ID: refreshes ID's row from the jar and keeps what is already known (DataVersion read from a world, jar_runs).
record() {
  local id=$1 json dv runs
  json="$(version_json "$id")"
  dv="$(tsv_get "$id" 2)"
  [ -n "$dv" ] && [ "$dv" != "-" ] || dv="$(jar_data_version "$CACHE/jars/$id.jar")"
  runs="$(tsv_get "$id" 5)"
  tsv_put "$id" "${dv:--}" "$(jq -r .downloads.server.sha1 "$json")" "$(jq -r '.javaVersion.majorVersion // 8' "$json")" "${runs:--}"
}

# ---- Java -------------------------------------------------------------------------------------------------------

# java_major EXE: the feature release of a java executable (1.8.0_512 is 8).
java_major() { "$1" -version 2>&1 | sed -n 's/.*version "\(1\.\)\{0,1\}\([0-9]*\).*/\2/p' | head -n1; }

# java_for MAJOR: prints a java executable that can run a jar built for MAJOR; fails with a message otherwise.
# Up to 8: only MC_OLD_JAVA (a JDK 8 home), never downloaded. Newer: the installed JDK with exactly MAJOR, else the
# smallest installed JDK above it.
java_for() {
  local want=$1 exe m best="" best_m=999
  if [ "$want" -le 8 ]; then
    exe="${MC_OLD_JAVA:+$MC_OLD_JAVA/bin/java}"
    if [ -x "${exe:-}" ] && [ "$(java_major "$exe")" = 8 ]; then echo "$exe"; return 0; fi
    echo "needs JDK 8: install one and set MC_OLD_JAVA=<its home>${MC_OLD_JAVA:+ (MC_OLD_JAVA=$MC_OLD_JAVA is not a JDK 8 home)}" >&2
    return 1
  fi
  for exe in "$HOME"/.jdks/*/bin/java /usr/lib/jvm/*/bin/java "$(command -v java || true)"; do
    [ -x "$exe" ] || continue
    m="$(java_major "$exe")"
    [ -n "$m" ] && [ "$m" -ge "$want" ] && [ "$m" -lt "$best_m" ] && { best="$exe"; best_m="$m"; }
  done
  [ -n "$best" ] && { echo "$best"; return 0; }
  echo "needs JDK $want or newer: none installed" >&2
  return 1
}

# ---- worlds -----------------------------------------------------------------------------------------------------

RUNNING=""
stop_running() { [ -z "$RUNNING" ] || "$VS" stop "$RUNNING" >/dev/null 2>&1 || true; RUNNING=""; }
trap stop_running EXIT

free_port() { python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }

# feed ID NAME FILE: sends the era script's commands to the server console (syntax in the header).
feed() {
  local id=$1 name=$2 file=$3 line rest v a b y=4
  ver_ge "$id" 1.18 && y=-60
  while IFS= read -r line || [ -n "$line" ]; do
    while :; do
      case $line in
        '@since '*) rest=${line#@since }; v=${rest%% *}; line=${rest#* }; ver_ge "$id" "$v" || line="" ;;
        '@until '*) rest=${line#@until }; v=${rest%% *}; line=${rest#* }; ver_ge "$id" "$v" && line="" ;;
        *) break ;;
      esac
      [ -n "$line" ] || break
    done
    case $line in
      '' | '#'*) continue ;;
      '@wait '*) read -r _ a b <<<"$line"; "$VS" wait "$name" "$a" "${b:-30}" >/dev/null || echo "datafix-worlds: $id: no log line /$a/" >&2 ;;
      '@sleep '*) sleep "${line#@sleep }" ;;
      *) "$VS" cmd "$name" "$(sed "s/@Y@/$y/g;s/@Y1@/$((y + 1))/g;s/@Y2@/$((y + 2))/g;s/@Y3@/$((y + 3))/g;s/@Y4@/$((y + 4))/g" <<<"$line")"; sleep "$DELAY" ;;
    esac
  done <"$file"
}

# start_world NAME ID JAVA LEVEL_TYPE RESET PROP...: starts the jar (RESET non-empty: a new world) and waits for it; when it
# does not come up the process is stopped, its output kept and 1 returned.
start_world() {
  local name=$1 id=$2 java=$3 level_type=$4 reset=$5; shift 5
  RUNNING="$name"
  "$VS" start "$name" ${reset:+--reset} --port "$(free_port)" --seed "$SEED" --level-type "$level_type" --view-distance 3 \
    --jar "$CACHE/jars/$id.jar" --java "$java" --xmx 2G -- "$@" && return 0
  mkdir -p "$CACHE/logs"
  cp "$ROOT/run/servers/$name/server.out" "$CACHE/logs/$id-${name##*-}.out" 2>/dev/null || true
  stop_running
  return 1
}

# gen_world ID KIND JAVA: runs the jar for one world (flat = scripted, normal = seed 12345 with structures), copies the saved
# world to CACHE/worlds/ID[-normal] and keeps the log. Returns 1 when the jar fails to start or dies.
gen_world() {
  local id=$1 kind=$2 java=$3 era name="dfw-$1-$2" level_type out srv log bad y=4 props=()
  era="$(era_of "$id")"
  out="$CACHE/worlds/$id"; [ "$kind" = flat ] || out="$out-normal"
  srv="$ROOT/run/servers/$name"
  ver_ge "$id" 1.18 && y=-60
  props=(max-tick-time=-1 enable-command-block=true)
  # 1.13 and older read gamemode and difficulty as numbers
  if ver_ge "$id" 1.14; then props+=(gamemode=creative difficulty=easy); else props+=(gamemode=1 difficulty=1); fi
  if [ "$kind" = flat ]; then
    props+=(generate-structures=false)
    if [ "$era" != e1 ] && [ "$era" != e2 ] && [ "$era" != e3 ]; then level_type=flat; ver_ge "$id" 1.19 && level_type=minecraft:flat; else level_type=FLAT; fi
  else
    props+=(generate-structures=true)
    if [ "$era" != e1 ] && [ "$era" != e2 ] && [ "$era" != e3 ]; then level_type=default; ver_ge "$id" 1.19 && level_type=minecraft:normal; else level_type=DEFAULT; fi
  fi
  start_world "$name" "$id" "$java" "$level_type" yes "${props[@]}" || return 1
  if [ "$kind" = flat ] && ! ver_ge "$id" 1.14; then
    # No usable /forceload here: the build area is the spawn area, which on a flat world lies up to 256 blocks from the origin.
    # Move it next to the scripted area and start again so the area is loaded.
    "$VS" cmd "$name" "setworldspawn 24 $y 24"; sleep 2
    "$VS" stop "$name" >/dev/null; RUNNING=""
    start_world "$name" "$id" "$java" "$level_type" "" "${props[@]}" || return 1
  fi
  if [ "$kind" = flat ]; then feed "$id" "$name" "$ERAS/$era.txt"; else feed "$id" "$name" "$ERAS/$era-normal.txt"; fi
  if ! "$VS" status "$name" >/dev/null; then
    echo "datafix-worlds: $id $kind: the server died while the script ran" >&2
    mkdir -p "$CACHE/logs"; cp "$srv/logs/latest.log" "$CACHE/logs/$id-$kind.log" 2>/dev/null || true
    stop_running; return 1
  fi
  "$VS" cmd "$name" "save-all"
  "$VS" wait "$name" 'Save complete|Saved the (world|game)' 120 >/dev/null || echo "datafix-worlds: $id $kind: no save confirmation" >&2
  "$VS" stop "$name" >/dev/null || return 1
  RUNNING=""
  mkdir -p "$CACHE/logs" "$CACHE/worlds"
  log="$CACHE/logs/$id-$kind.log"
  cp "$srv/logs/latest.log" "$log" || return 1
  bad="$(grep -ciE 'unknown|usage:|incorrect argument|expected |invalid|unable to|cannot|can.t|couldn.t|could not|not a valid|no such|no entity|no blocks|failed|not loaded|does not|unexpected error' "$log" || true)"
  [ "$bad" = 0 ] || echo "datafix-worlds: $id $kind: $bad console lines look like failed commands, see $log" >&2
  rm -rf "$out.part"
  cp -a "$srv/world" "$out.part" || return 1
  rm -rf "$out"
  mv "$out.part" "$out" || return 1
  rm -rf "$srv"
}

# write_hashes ID: sha256 of every file of ID's worlds, paths relative to CACHE/worlds, session.lock left out.
write_hashes() {
  local id=$1 list dirs=("$1")
  [ -d "$CACHE/worlds/$id" ] || { echo "datafix-worlds: no world for $id in the cache" >&2; return 0; }
  [ ! -d "$CACHE/worlds/$id-normal" ] || dirs+=("$id-normal")
  mkdir -p "$FIXTURES/world_hashes"
  list="$(cd "$CACHE/worlds" && find "${dirs[@]}" -type f ! -name session.lock | LC_ALL=C sort | xargs -d '\n' sha256sum)"
  printf '%s\n' "$list" >"$FIXTURES/world_hashes/$id.txt"
  cp "$FIXTURES/world_hashes/$id.txt" "$CACHE/worlds/$id.sha256"
}

# ---- main -------------------------------------------------------------------------------------------------------

mode=generate force=0 ids=()
while [ $# -gt 0 ]; do
  case $1 in
    --list) mode=list ;;
    --download) mode=download ;;
    --hashes) mode=hashes ;;
    --force) force=1 ;;
    -*) die "unknown option $1" ;;
    *) ids+=("$1") ;;
  esac
  shift
done
explicit=1
[ ${#ids[@]} -gt 0 ] || { explicit=0; ids=("${LISTED[@]}"); }

if [ "$mode" = list ]; then
  printf '%-10s %-4s %-5s %s\n' id era java jar
  for id in "${ids[@]}"; do
    major="?"
    [ ! -s "$CACHE/json/$id.json" ] || major="$(jq -r '.javaVersion.majorVersion // 8' "$CACHE/json/$id.json")"
    have="-"; [ ! -f "$CACHE/jars/$id.jar" ] || have="cached"
    printf '%-10s %-4s %-5s %s\n' "$id" "$(era_of "$id")" "$major" "$have"
  done
  exit 0
fi

if [ "$mode" = hashes ]; then
  for id in "${ids[@]}"; do write_hashes "$id"; done
  exit 0
fi

[ "$mode" = download ] || require_eula
skipped=0
for id in "${ids[@]}"; do
  era_of "$id" >/dev/null
  fetch_jar "$id"
  record "$id"
  [ "$mode" = download ] && continue
  if [ -d "$CACHE/worlds/$id" ] && [ -d "$CACHE/worlds/$id-normal" ] && [ "$force" = 0 ]; then
    echo "datafix-worlds: $id: worlds exist (--force regenerates them)"; continue
  fi
  major="$(jq -r '.javaVersion.majorVersion // 8' "$(version_json "$id")")"
  if ! java="$(java_for "$major" 2>&1)"; then
    echo "datafix-worlds: skip $id: $java" >&2
    [ "$explicit" = 0 ] || skipped=1
    continue
  fi
  echo "datafix-worlds: $id ($(era_of "$id"), $java)"
  if gen_world "$id" flat "$java" && gen_world "$id" normal "$java"; then
    runs=true
    dv="$(level_data_version "$CACHE/worlds/$id/level.dat")" || dv=-
  else
    runs=false dv="$(tsv_get "$id" 2)"
    echo "datafix-worlds: $id: the jar did not start or died, see $CACHE/logs/$id-*" >&2
  fi
  tsv_put "$id" "${dv:--}" "$(tsv_get "$id" 3)" "$(tsv_get "$id" 4)" "$runs"
  [ "$runs" = false ] || write_hashes "$id"
done
[ "$skipped" = 0 ] || exit 3
