#!/usr/bin/env bash
# Machine-wide JVM memory slots for the minecraft project's oracles (vanilla servers, oracle programs, real client).
# Installed as ~/.local/bin/java (ahead of /usr/bin on PATH) by scripts/install-java-slot.sh, so every agent's `java`
# call is throttled without changing callers. Only JVMs whose arguments reference this project are throttled; any other
# java use passes straight through. The real client counts as 2 slots. systemd-oomd killed the session twice when too
# many builds/JVMs overlapped.
REAL_JAVA="${MC_REAL_JAVA:-/usr/lib/jvm/default/bin/java}"
SLOTS="${MC_JVM_SLOTS:-2}"

case "$*" in
  */Projects/minecraft/*|*/Projects/mc-wt/*|*mc-wt*|*server.jar*) ;;
  *) exec "$REAL_JAVA" "$@" ;;
esac
need=1
case "$*" in *net.minecraft.client.main.Main*|*E2eMain*) need=2 ;; esac

# Offload: oracle programs / generators run on a cloud builder (scripts/builders/remote-java.sh mirrors the worktree,
# streams I/O and syncs written files back). Servers (they listen on local ports), the real client, custom JDKs
# (MC_REAL_JAVA, e.g. old-version servers) and JVMs already on a builder stay local.
REG=/tmp/mc-builders
if [ -z "${MC_ON_BUILDER:-}" ] && [ -z "${MC_REAL_JAVA:-}" ] && [ "$need" = 1 ] && [ -n "${MC_JAVA_REMOTE:-}" ]; then
  case "$*" in *server.jar*|*nogui*|*paper*.jar*) ;; *)
    rj="$(dirname "$(readlink -f "$0")")/builders/remote-java.sh"
    deadline=$(( $(date +%s) + ${MC_REMOTE_JVM_WAIT_S:-1800} ))
    while ls "$REG"/*.ip >/dev/null 2>&1 && [ "$(date +%s)" -lt "$deadline" ]; do
      for reg in "$REG"/*.ip; do
        name=$(basename "$reg" .ip)
        for s in $(seq 1 "${MC_REMOTE_JVM_SLOTS:-3}"); do
          exec {rfd}>"/tmp/mc-remote.$name.jvm$s"
          if flock -n "$rfd"; then
            touch "$REG/$name.last"; "$rj" "$(cat "$reg")" "$s" "$@"; rc=$?; touch "$REG/$name.last"
            [ "$rc" = 255 ] || exit $rc
            exec {rfd}>&-; break 3   # transport failure: run locally below
          fi
          exec {rfd}>&-
        done
      done
      sleep 3
    done ;;
  esac
fi

held=()
while :; do
  held=()
  for i in $(seq 1 "$SLOTS"); do
    exec {fd}>"/tmp/mc-jvm.lock.$i"
    if flock -n "$fd"; then held+=("$fd"); [ "${#held[@]}" -ge "$need" ] && break; else exec {fd}>&-; fi
  done
  [ "${#held[@]}" -ge "$need" ] && break
  for fd in "${held[@]}"; do exec {fd}>&-; done
  sleep 3
done
# The lock fds stay open in the exec'd JVM, so the slots are held exactly as long as the JVM lives.
exec "$REAL_JAVA" "$@"
