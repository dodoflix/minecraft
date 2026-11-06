#!/usr/bin/env bash
# Run the real vanilla 26.3 client headlessly (gamescope --backend headless + its private Xwayland;
# the desktop's DISPLAY/WAYLAND_DISPLAY are never used), offline account, audio off, tiny render distance.
#
#   real-client.sh prepare                                  download client/libs/assets into run/client-cache (idempotent)
#   real-client.sh start NAME USERNAME HOST:PORT [opts]     launch, auto-joins via --quickPlayMultiplayer, returns immediately
#   real-client.sh screenshot NAME [OUT.png]                F2 inside the client; prints the saved PNG path
#   real-client.sh status NAME                              JSON verdict parsed from the client log (see mc-client-status.py)
#   real-client.sh wait NAME REGEX [TIMEOUT_S=60]           wait for a regex in the client log
#   real-client.sh key NAME KEYSYM...                       send keys to the client window (xdotool syntax, e.g. t, Escape)
#   real-client.sh log NAME                                 print log path
#   real-client.sh stop NAME                                close the client (SIGTERM, then SIGKILL)
#
# start opts: --width N (854) --height N (480) --render-distance N (4) --xmx 2G --option key:value (extra options.txt line)
# State lives in run/clients/NAME/{game,client.out,pid,display}. client.out = JVM stdout+stderr = the client log
# (the stock client has no log file without the launcher's log4j config). Each start wipes old logs/screenshots/crash reports.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CACHE="$ROOT/run/client-cache"
die() { echo "real-client: $*" >&2; exit 1; }
D_OF() { echo "$ROOT/run/clients/$1"; }
alive() { local p; p="$(cat "$(D_OF "$1")/pid" 2>/dev/null || true)"; [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null; }
display_of() { cat "$(D_OF "$1")/display" 2>/dev/null || die "client $1 has no display yet (not started?)"; }

prepare() { python3 "$ROOT/scripts/mc-client-prepare.py" "$CACHE" 2>"$ROOT/run/client-prepare.log"; }

cmd="${1:-}"; shift || true
if [[ "$cmd" == prepare ]]; then prepare >/dev/null; echo "client ready in $CACHE"; exit 0; fi
name="${1:-}"; shift || true
[[ -n "$cmd" && -n "$name" ]] || die "usage: $0 prepare|start|screenshot|status|wait|key|log|stop NAME ..."
D="$(D_OF "$name")"

case "$cmd" in
start)
  user="${1:?USERNAME}"; addr="${2:?HOST:PORT}"; shift 2
  w=854 h=480 rd=4 xmx=2G; extra=()
  while (($#)); do
    case "$1" in
      --width) w="$2"; shift 2;;
      --height) h="$2"; shift 2;;
      --render-distance) rd="$2"; shift 2;;
      --xmx) xmx="$2"; shift 2;;
      --option) extra+=("$2"); shift 2;;
      *) die "unknown option $1";;
    esac
  done
  ! alive "$name" || die "client $name already running"
  CP="$(prepare)"
  # E2eMain wraps the stock Main and prints observable client state ("[E2E] {json}"); see scripts/e2e/E2eMain.java
  SHIM="$CACHE/e2e-shim"
  if [[ ! -f "$SHIM/E2eMain.class" || "$ROOT/scripts/e2e/E2eMain.java" -nt "$SHIM/E2eMain.class" ]]; then
    mkdir -p "$SHIM" && javac -proc:none -cp "$CP" -d "$SHIM" "$ROOT/scripts/e2e/E2eMain.java"
  fi
  rm -rf "$D/game/logs" "$D/game/screenshots" "$D/game/crash-reports" "$D/client.out" "$D/display"
  mkdir -p "$D/game" "$D/natives"
  {
    echo "renderDistance:$rd"
    echo "simulationDistance:5"
    echo "pauseOnLostFocus:false"
    echo "onboardAccessibility:false"
    echo "onboardingAccessibilityFinished:true"
    echo "skipMultiplayerWarning:true"
    echo "joinedFirstServer:true"
    echo "tutorialStep:none"
    echo "narrator:0"
    echo "musicToast:\"never\""
    echo "showSubtitles:false"
    echo "maxFps:30"
    echo "enableVsync:false"
    echo "fullscreen:false"
    echo "telemetryOptInExtra:false"
    echo "soundCategory_master:0.0"
    for c in music record weather block hostile neutral player ambient voice ui; do echo "soundCategory_$c:0.0"; done
    for l in "${extra[@]}"; do echo "$l"; done
  } > "$D/game/options.txt"
  # The wrapper runs inside gamescope so it sees gamescope's private Xwayland DISPLAY.
  cat > "$D/launch.sh" <<LAUNCH
#!/usr/bin/env bash
echo "\$DISPLAY" > "$D/display"
export ALSOFT_DRIVERS=null   # OpenAL Soft null backend: audio initialises but goes nowhere
exec nice -n 10 java -XX:ActiveProcessorCount=4 -Xms512M -Xmx$xmx -XX:+UseZGC -XX:StackShadowPages=32 --enable-native-access=ALL-UNNAMED \\
  --add-exports java.base/jdk.internal.misc=ALL-UNNAMED \\
  -Djava.library.path="$D/natives/java" -Djna.tmpdir="$D/natives/jna" \\
  -Dorg.lwjgl.system.SharedLibraryExtractPath="$D/natives/lwjgl" -Dio.netty.native.workdir="$D/natives/netty" \\
  -Dminecraft.api.env=custom -Dminecraft.api.auth.host=http://127.0.0.1:9 -Dminecraft.api.account.host=http://127.0.0.1:9 \\
  -Dminecraft.api.session.host=http://127.0.0.1:9 -Dminecraft.api.services.host=http://127.0.0.1:9 \\
  -Dminecraft.launcher.brand=e2e -Dminecraft.launcher.version=1 \\
  -cp "$CP:$SHIM" E2eMain \\
  --username "$user" --uuid "\$(python3 -c 'import uuid,sys;print(uuid.uuid3(uuid.NAMESPACE_OID,"OfflinePlayer:"+sys.argv[1]).hex)' "$user")" \\
  --accessToken 0 --version 26.3 --versionType release --gameDir "$D/game" \\
  --assetsDir "$CACHE/assets" --assetIndex "$(python3 -c "import json;print(json.load(open('$ROOT/ref/26.3.json'))['assetIndex']['id'])")" \\
  --width $w --height $h --quickPlayMultiplayer "$addr"
LAUNCH
  chmod +x "$D/launch.sh"
  # env -u: gamescope's headless backend needs no display; make sure we cannot touch the desktop.
  setsid env -u DISPLAY -u WAYLAND_DISPLAY gamescope --backend headless -W "$w" -H "$h" -w "$w" -h "$h" \
    -- "$D/launch.sh" > "$D/client.out" 2>&1 < /dev/null &
  echo $! > "$D/pid"
  for _ in $(seq 100); do [[ -s "$D/display" ]] && break; sleep 0.1; done
  echo "started pid=$(cat "$D/pid") display=$(cat "$D/display" 2>/dev/null) log=$D/client.out"
  ;;
screenshot)
  out="${1:-}"; sdir="$D/game/screenshots"; mkdir -p "$sdir"
  count() { find "$sdir" -mindepth 1 -maxdepth 1 | wc -l; }
  before="$(count)"
  DISPLAY="$(display_of "$name")" xdotool key --clearmodifiers F2
  for _ in $(seq 60); do
    [[ "$(count)" -gt "$before" ]] && break; sleep 0.2
  done
  [[ "$(count)" -gt "$before" ]] || die "no screenshot appeared in $sdir"
  f="$(find "$sdir" -mindepth 1 -maxdepth 1 -printf '%T@ %p\n' | sort -rn | head -1 | cut -d' ' -f2-)"
  sleep 0.3  # file is written by a worker thread
  if [[ -n "$out" ]]; then mkdir -p "$(dirname "$out")"; cp "$f" "$out"; echo "$out"; else echo "$f"; fi
  ;;
status) python3 "$ROOT/scripts/mc-client-status.py" "$D" ;;
wait)
  re="${1:?regex}"; t="${2:-60}"
  for _ in $(seq $((t * 5))); do
    grep -qE -- "$re" "$D/client.out" 2>/dev/null && { grep -E -- "$re" "$D/client.out" | tail -1; exit 0; }
    alive "$name" || die "client exited while waiting for /$re/"
    sleep 0.2
  done
  die "timeout waiting for /$re/"
  ;;
key) DISPLAY="$(display_of "$name")" xdotool key --clearmodifiers "$@" ;;
log) echo "$D/client.out" ;;
stop)
  p="$(cat "$D/pid" 2>/dev/null || true)"
  if [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null; then
    # gamescope exits when its child exits; SIGTERM to gamescope terminates the child tree.
    pkill -TERM -P "$p" 2>/dev/null || true; kill -TERM "$p" 2>/dev/null || true
    for _ in $(seq 150); do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done
    if kill -0 "$p" 2>/dev/null; then echo "stop timed out, killing" >&2; pkill -KILL -g "$p" 2>/dev/null || kill -9 "$p"; fi
  fi
  rm -f "$D/pid"; echo stopped
  ;;
*) die "unknown command $cmd" ;;
esac
