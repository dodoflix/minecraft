#!/usr/bin/env bash
# Send one control command to a running real client and print its reply.
#
#   client-ctl.sh NAME JSON [--timeout S=15]       JSON = {"cmd": "chat", "text": "hi"}; an "id" is added when missing
#   client-ctl.sh NAME snapshot [OUT.json]         shorthand for the snapshot command (the reply has the file path)
#
# The line goes to the FIFO run/clients/NAME/ctl, the client executes it on its render thread and answers with
# "[E2E-CTL] {id, ok, result|error}" in client.out. The reply object is printed; exit 0 iff ok, 1 when the command
# failed, 2 on timeout or a client that is gone. Commands and the snapshot format: tests/e2e/client/PROTOCOL.md.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need python3 timeout
name="${1:?NAME}"; json="${2:?JSON or snapshot}"; shift 2; out=""
t=15
while (($#)); do
  case "$1" in
    --timeout) t="$2"; shift 2;;
    *) if [[ "$json" == snapshot && -z "$out" ]]; then out="$1"; shift; else die "unknown option $1"; fi;;
  esac
done
D="$ROOT/run/clients/$name"
[[ -p "$D/ctl" ]] || { echo "client-ctl: client $name has no control channel (not started?)" >&2; exit 2; }
running() { local p; p="$(cat "$D/pid" 2>/dev/null || true)"; [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null; }
running || { echo "client-ctl: client $name is not running" >&2; exit 2; }
if [[ "$json" == snapshot ]]; then
  json='{"cmd":"snapshot"}'
  [[ -z "$out" ]] || json="$(python3 -c 'import json,sys;print(json.dumps({"cmd":"snapshot","path":sys.argv[1]}))' "$(realpath -m "$out")")"
fi
line="$(python3 -c 'import json,sys;o=json.loads(sys.argv[1]);o.setdefault("id","c%d-%d"%(int(sys.argv[2]),int(sys.argv[3])));print(json.dumps(o))' \
  "$json" "$$" "$RANDOM")"
idj="$(python3 -c 'import json,sys;print(json.dumps(json.loads(sys.argv[1])["id"]))' "$line")"
skip="$(wc -l < "$D/client.out")"
# shellcheck disable=SC2016  # the inner script expands in the child shell
timeout 5 bash -c 'printf "%s\n" "$1" > "$2"' _ "$line" "$D/ctl" || { echo "client-ctl: nothing reads $D/ctl" >&2; exit 2; }
for _ in $(seq $((t * 10))); do
  reply="$(tail -n +"$((skip + 1))" "$D/client.out" | grep -F "[E2E-CTL] {\"id\":$idj," | head -1 || true)"
  if [[ -n "$reply" ]]; then
    reply="${reply#*\[E2E-CTL\] }"
    echo "$reply"
    python3 -c 'import json,sys;sys.exit(0 if json.loads(sys.argv[1])["ok"] else 1)' "$reply"
    exit $?
  fi
  running || { echo "client-ctl: client exited" >&2; exit 2; }
  sleep 0.1
done
echo "client-ctl: no reply within ${t}s" >&2
exit 2
