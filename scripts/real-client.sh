#!/usr/bin/env bash
# Run the real vanilla 26.3 client headlessly (gamescope --backend headless + its private Xwayland, or Xvfb;
# the desktop's DISPLAY/WAYLAND_DISPLAY are never used), offline account, audio off, tiny render distance.
#
#   real-client.sh prepare                                  download client/libs/assets into run/client-cache (idempotent)
#   real-client.sh start NAME USERNAME HOST:PORT [opts]     launch, auto-joins via --quickPlayMultiplayer, returns immediately
#   real-client.sh start NAME USERNAME none [opts]          same without auto-join (title screen; see --server)
#   real-client.sh screenshot NAME [OUT.png]                PNG of the current frame (control channel); prints the path
#   real-client.sh status NAME                              JSON verdict parsed from the client log (see mc-client-status.py)
#   real-client.sh wait NAME REGEX [TIMEOUT_S=60]           wait for a regex in the client log
#   real-client.sh key NAME KEYSYM...                       send keys to the client window (xdotool syntax, e.g. t, Escape)
#   real-client.sh log NAME                                 print log path
#   real-client.sh stop NAME                                close the client (SIGTERM, then SIGKILL)
#
# start opts: --width N (854) --height N (480) --render-distance N (4) --xmx 2G --option key:value (extra options.txt line)
#             --mojang URL (discovery document of the services: http://127.0.0.1:9/minecraft/client, a closed local port,
#                           so the services are offline; pass a local mock to serve sessions and profile keys)
#             --display gamescope|xvfb (gamescope: headless compositor with a GPU; xvfb: Xvfb + software GL, needs Xvfb)
#             --server NAME=HOST:PORT (repeatable: writes the server list, servers.dat, in this order)
#             --keep-options (keep an existing options.txt instead of writing the deterministic profile)
# The profile: fixed render settings (clouds off, minimal particles, no entity shadows, GUI scale 2, gamma 0.5,
# en_us, no view bobbing or screen effects) so frames and snapshots repeat; --option lines come last and win.
# The client log4j configuration (scripts/client-log4j2.xml) writes game/latest.log with DEBUG for the network packages.
# The access token is the username (a mock Mojang service identifies the profile by it).
# Control: run/clients/NAME/ctl is a FIFO read by the client (scripts/client-ctl.sh, tests/e2e/client/PROTOCOL.md).
# Isolation: the client never talks to Mojang. authlib reads only -Dminecraft.api.discovery.host (every services URL
# comes from that document; the other -Dminecraft.api.*.host flags are ignored). The Realms host is set to
# localhost:8080 (-Drealms.environment=local) and patchy's hard-coded block list fetch is taken off the classpath.
# E2eResolver refuses every other name but loopback, the machine's own and the game server's; each refused name is
# printed as an "[E2E-NET]" line and fails the verdict. net-watch.py records sockets to non-loopback addresses
# (literal IPs bypass the resolver). status shows both under "egress".
# State lives in run/clients/NAME/{game,client.out,pid,display,ctl}. client.out = JVM stdout+stderr = the client log
# at INFO. Each start wipes old logs/screenshots/crash reports/snapshots.
# Needs: python3; java, javac, gamescope (or Xvfb), xdotool, ss for start/key. The run/ directory is created on first use.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
CACHE="$ROOT/run/client-cache"
die() { echo "real-client: $*" >&2; exit 1; }
D_OF() { echo "$ROOT/run/clients/$1"; }
alive() { local p; p="$(cat "$(D_OF "$1")/pid" 2>/dev/null || true)"; [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null; }
display_of() { cat "$(D_OF "$1")/display" 2>/dev/null || die "client $1 has no display yet (not started?)"; }

need python3
mkdir -p "$ROOT/run"
prepare() { python3 "$ROOT/scripts/mc-client-prepare.py" "$CACHE" 2>"$ROOT/run/client-prepare.log"; }

cmd="${1:-}"; shift || true
if [[ "$cmd" == prepare ]]; then prepare >/dev/null; echo "client ready in $CACHE"; exit 0; fi
name="${1:-}"; shift || true
[[ -n "$cmd" && -n "$name" ]] || { usage >&2; exit 1; }
[[ "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "bad NAME: $name"
D="$(D_OF "$name")"

case "$cmd" in
start)
  need java javac python3 xdotool setsid ss mkfifo
  user="${1:?USERNAME}"; addr="${2:?HOST:PORT or none}"; shift 2
  w=854 h=480 rd=4 xmx=2G; extra=(); servers=(); keep_options=0 display=gamescope
  mojang="http://127.0.0.1:9/minecraft/client"
  while (($#)); do
    case "$1" in
      --width) w="$2"; shift 2;;
      --height) h="$2"; shift 2;;
      --render-distance) rd="$2"; shift 2;;
      --xmx) xmx="$2"; shift 2;;
      --option) extra+=("$2"); shift 2;;
      --mojang) mojang="$2"; shift 2;;
      --display) display="$2"; shift 2;;
      --server) servers+=("$2"); shift 2;;
      --keep-options) keep_options=1; shift;;
      *) die "unknown option $1";;
    esac
  done
  case "$display" in
    gamescope) need gamescope;;
    xvfb) need Xvfb;;
    *) die "--display is gamescope or xvfb, not $display";;
  esac
  # the client asks DNS for a SRV record of the host whenever the port is 25565 (outside the resolver below)
  check_port() {
    [[ "$1" =~ :([0-9]+)$ ]] || die "HOST:PORT needs an explicit port (the client defaults to 25565): $1"
    ((10#${BASH_REMATCH[1]} != 25565)) || die "port 25565 makes the client query DNS for _minecraft._tcp.HOST: use another port"
  }
  if [[ "$addr" == none ]]; then
    quick=() host=""
  else
    check_port "$addr"
    quick=(--quickPlayMultiplayer "$addr")
    host="${addr%:*}"; host="${host#[}"; host="${host%]}"
  fi
  ! alive "$name" || die "client $name already running"
  CP="$(prepare)"
  # patchy's MojangBlockListSupplier (found by ServiceLoader when a connection starts) fetches
  # https://sessionserver.mojang.com/blockedservers: a hard-coded host that no -D flag changes. Without it the client
  # blocks no address.
  CP="$(tr ':' '\n' <<<"$CP" | grep -v '/com/mojang/patchy/' | paste -sd:)"
  # the shim (E2eMain wraps the stock Main: probe, control channel, resolver) is compiled against the client jar
  SHIM="$CACHE/e2e-shim"
  stale=0
  [[ -f "$SHIM/E2eMain.class" && -f "$SHIM/E2eResolver.class" ]] || stale=1
  for f in "$ROOT"/scripts/e2e/*.java; do [[ "$f" -nt "$SHIM/E2eMain.class" ]] && stale=1; done
  if ((stale)); then
    rm -rf "$SHIM"; mkdir -p "$SHIM/META-INF/services"
    javac -proc:none -cp "$CP" -d "$SHIM" "$ROOT"/scripts/e2e/*.java
    echo E2eResolver > "$SHIM/META-INF/services/java.net.spi.InetAddressResolverProvider"
  fi
  # the game server's own address: the resolver lets its name through (a literal address is never looked up) and
  # net-watch accepts connections to it
  allow_hosts="$host" watch_allow=()
  [[ -z "$host" ]] || watch_allow=(--allow "$host")
  if [[ -n "$host" && ! "$host" =~ ^[0-9.]+$ && "$host" != *:* ]]; then
    while read -r ip; do watch_allow+=(--allow "$ip"); done < <(python3 -c \
      'import socket,sys;print(*{a[4][0] for a in socket.getaddrinfo(sys.argv[1],None)},sep="\n")' "$host" 2>/dev/null || true)
  fi
  x="$(cat "$D/xvfb.pid" 2>/dev/null || true)"; [[ -z "$x" ]] || kill -TERM "$x" 2>/dev/null || true   # left by a client that died
  rm -rf "$D/game/logs" "$D/game/screenshots" "$D/game/crash-reports" "$D/game/debug" "$D/game/e2e" "$D/game/latest.log" \
    "$D/client.out" "$D/display" "$D/net-egress.log" "$D/ctl" "$D/xvfb.pid"
  mkdir -p "$D/game" "$D/natives"
  mkfifo "$D/ctl"
  for s in "${servers[@]}"; do check_port "${s#*=}"; done
  if ((${#servers[@]})); then python3 "$ROOT/scripts/mc-client-prepare.py" --servers-dat "$D/game/servers.dat" "${servers[@]}"; fi
  if ((!keep_options)) || [[ ! -f "$D/game/options.txt" ]]; then
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
      echo "renderClouds:\"false\""
      echo "particles:2"
      echo "entityShadows:false"
      echo "guiScale:2"
      echo "gamma:0.5"
      echo "lang:en_us"
      echo "bobView:false"
      echo "screenEffectScale:0.0"
      echo "fovEffectScale:0.0"
      echo "darknessEffectScale:0.0"
      echo "glintSpeed:0.0"
      echo "showAutosaveIndicator:false"
      echo "soundCategory_master:0.0"
      for c in music record weather block hostile neutral player ambient voice ui; do echo "soundCategory_$c:0.0"; done
      for l in "${extra[@]}"; do echo "$l"; done
    } > "$D/game/options.txt"
  fi
  # The wrapper runs inside the display (gamescope's private Xwayland, or Xvfb) so it sees that DISPLAY.
  cat > "$D/launch.sh" <<LAUNCH
#!/usr/bin/env bash
echo "\$DISPLAY" > "$D/display"
export ALSOFT_DRIVERS=null   # OpenAL Soft null backend: audio initialises but goes nowhere
exec nice -n 10 java -XX:ActiveProcessorCount=4 -Xms512M -Xmx$xmx -XX:+UseZGC -XX:StackShadowPages=32 --enable-native-access=ALL-UNNAMED \\
  --add-exports java.base/jdk.internal.misc=ALL-UNNAMED \\
  -Djava.library.path="$D/natives/java" -Djna.tmpdir="$D/natives/jna" \\
  -Dorg.lwjgl.system.SharedLibraryExtractPath="$D/natives/lwjgl" -Dio.netty.native.workdir="$D/natives/netty" \\
  -Dminecraft.api.discovery.host="$mojang" -De2e.allow-hosts="$allow_hosts" \\
  -De2e.local-host="$(cat /proc/sys/kernel/hostname 2>/dev/null || true)" -Drealms.environment=local \\
  -Dlog4j.configurationFile="$ROOT/scripts/client-log4j2.xml" -De2e.game-dir="$D/game" -De2e.ctl="$D/ctl" \\
  -Dminecraft.launcher.brand=e2e -Dminecraft.launcher.version=1 \\
  -cp "$CP:$SHIM" E2eMain \\
  --username "$user" --uuid "\$(python3 -c 'import uuid,sys;print(uuid.uuid3(uuid.NAMESPACE_OID,"OfflinePlayer:"+sys.argv[1]).hex)' "$user")" \\
  --accessToken "$user" --version 26.3 --versionType release --gameDir "$D/game" \\
  --assetsDir "$CACHE/assets" --assetIndex "$(python3 -c "import json;print(json.load(open('$ROOT/ref/26.3.json'))['assetIndex']['id'])")" \\
  --width $w --height $h ${quick[*]}
LAUNCH
  chmod +x "$D/launch.sh"
  if [[ "$display" == xvfb ]]; then
    # -displayfd reports the display number once the server accepts connections; the client renders in software
    setsid Xvfb -displayfd 3 -screen 0 "${w}x${h}x24" -nolisten tcp 3> "$D/xvfb.display" > "$D/xvfb.log" 2>&1 < /dev/null &
    echo $! > "$D/xvfb.pid"
    for _ in $(seq 100); do [[ -s "$D/xvfb.display" ]] && break; sleep 0.1; done
    [[ -s "$D/xvfb.display" ]] || die "Xvfb did not start, see $D/xvfb.log"
    setsid env -u WAYLAND_DISPLAY DISPLAY=":$(tr -d '\n' < "$D/xvfb.display")" LIBGL_ALWAYS_SOFTWARE=1 SDL_VIDEO_DRIVER=x11 \
      "$D/launch.sh" > "$D/client.out" 2>&1 < /dev/null &
  else
    # env -u: gamescope's headless backend needs no display; make sure we cannot touch the desktop.
    setsid env -u DISPLAY -u WAYLAND_DISPLAY gamescope --backend headless -W "$w" -H "$h" -w "$w" -h "$h" \
      -- "$D/launch.sh" > "$D/client.out" 2>&1 < /dev/null &
  fi
  echo $! > "$D/pid"
  python3 "$ROOT/scripts/e2e/net-watch.py" "$D" "${watch_allow[@]}" > /dev/null 2>&1 < /dev/null &
  for _ in $(seq 100); do [[ -s "$D/display" ]] && break; sleep 0.1; done
  echo "started pid=$(cat "$D/pid") display=$(cat "$D/display" 2>/dev/null) log=$D/client.out"
  ;;
screenshot)
  out="${1:-}"
  [[ -n "$out" ]] || out="$D/game/screenshots/$(date +%Y%m%d_%H%M%S).png"
  mkdir -p "$(dirname "$out")"; out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
  "$ROOT/scripts/client-ctl.sh" "$name" "{\"cmd\":\"screenshot\",\"path\":\"$out\"}" > /dev/null
  echo "$out"
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
  x="$(cat "$D/xvfb.pid" 2>/dev/null || true)"
  [[ -z "$x" ]] || kill -TERM "$x" 2>/dev/null || true
  rm -f "$D/pid" "$D/xvfb.pid" "$D/ctl"; echo stopped
  ;;
*) die "unknown command $cmd" ;;
esac
