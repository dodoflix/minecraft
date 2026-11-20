#!/usr/bin/env python3
"""Record every socket of the e2e client that talks to a non-loopback address.

Usage: scripts/e2e/net-watch.py RUN_DIR [--allow ADDRESS]...

Polls `ss` every 200 ms while the client of RUN_DIR (run/clients/NAME) runs and appends one line `PROTO PEER` per new
non-loopback peer of the java process of this run (its command line names E2eMain and RUN_DIR) to
RUN_DIR/net-egress.log. The names a client resolves are refused by scripts/e2e/E2eResolver.java; this covers
connections to literal addresses. `--allow` names the game server's address when it is not on the loopback
interface. A connection that opens and closes between two polls is not seen.
"""
import os
import re
import subprocess
import sys
import time

if sys.argv[1:2] in (["-h"], ["--help"]) or len(sys.argv) < 2:
    print(__doc__)
    sys.exit(0)

run_dir = os.path.abspath(sys.argv[1])
allow = {sys.argv[i + 1] for i, a in enumerate(sys.argv) if a == "--allow"}
LOOPBACK = re.compile(r"^(127\.|::1$)")
seen = set()


def client_alive():
    try:
        os.kill(int(open(os.path.join(run_dir, "pid")).read()), 0)
        return True
    except (OSError, ValueError):
        return False


def is_client(pid):
    """A java process of this run: it names E2eMain and the run's game directory."""
    try:
        cmdline = open(f"/proc/{pid}/cmdline", "rb").read()
    except OSError:
        return False
    return b"E2eMain" in cmdline and (run_dir + "/").encode() in cmdline


while client_alive():
    out = subprocess.run(["ss", "-H", "-tunp"], capture_output=True, text=True).stdout
    for line in out.splitlines():
        cols = line.split()
        pid = re.search(r"pid=(\d+)", line)
        if len(cols) < 6 or not pid or not is_client(pid.group(1)):
            continue
        peer = cols[5]
        # a dual-stack socket shows an IPv4 peer as [::ffff:a.b.c.d]
        host = re.sub(r"^\[::ffff:(.*)\]$", r"\1", peer.rsplit(":", 1)[0]).strip("[]")
        # an unconnected UDP socket has no peer (`0.0.0.0:*`)
        if peer.endswith(":*") or LOOPBACK.match(host) or host in allow or peer in allow:
            continue
        if (cols[0], peer) not in seen:
            seen.add((cols[0], peer))
            with open(os.path.join(run_dir, "net-egress.log"), "a") as log:
                log.write(f"{cols[0]} {peer}\n")
    time.sleep(0.2)
