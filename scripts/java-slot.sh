#!/usr/bin/env bash
# Machine-wide JVM memory slots for the minecraft project's oracles (vanilla servers, oracle programs, real client).
# Installed as ~/.local/bin/java (ahead of /usr/bin on PATH) by scripts/install-java-slot.sh, so every agent's `java`
# call is throttled without changing callers. Only JVMs whose arguments reference this project are throttled; any other
# java use passes straight through. The real client counts as 2 slots. systemd-oomd killed the session twice when too
# many builds/JVMs overlapped.
REAL_JAVA="${MC_REAL_JAVA:-/usr/lib/jvm/default/bin/java}"
SLOTS="${MC_JVM_SLOTS:-3}"

case "$*" in
  */Projects/minecraft/*|*/Projects/mc-wt/*|*mc-wt*|*server.jar*) ;;
  *) exec "$REAL_JAVA" "$@" ;;
esac
need=1
case "$*" in *net.minecraft.client.main.Main*|*E2eMain*) need=2 ;; esac

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
