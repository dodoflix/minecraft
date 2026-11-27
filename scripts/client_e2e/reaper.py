"""Stops what a runner started when the runner itself dies without cleaning up (SIGKILL, harness cancel, OOM kill).

The runner registers every server, client and mock it starts in a journal; this process, started in its own session,
waits until the runner is gone and then stops whatever is still registered. A normal run unregisters everything
itself, so the reaper finds nothing and exits.

    python3 -m client_e2e.reaper RUNNER_PID JOURNAL
"""
import json
import os
import signal
import subprocess
import sys
import time


def _start_time(pid):
    """Field 22 of /proc/PID/stat (None when the process is gone or a zombie): tells a reused pid from the runner."""
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as f:
            rest = f.read().rpartition(")")[2].split()
    except OSError:
        return None
    return None if rest[0] == "Z" else rest[19]


class Journal:
    """Append-only lines `{"key", "stop": argv}` (run argv to stop it) or `{"key", "pid", "cwd"}` (kill the process group of
    pid while its cwd is still `cwd`), and `{"drop": key}` once the runner stopped it."""

    def __init__(self, path):
        self.path = path

    def _append(self, obj):
        fd = os.open(self.path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
        try:
            os.write(fd, (json.dumps(obj) + "\n").encode())
        finally:
            os.close(fd)

    def add(self, key, **entry):
        self._append({"key": key, **entry})

    def drop(self, key):
        self._append({"drop": key})

    def live(self):
        entries = {}
        try:
            with open(self.path, encoding="utf-8") as f:
                for line in f:
                    obj = json.loads(line)
                    if "drop" in obj:
                        entries.pop(obj["drop"], None)
                    else:
                        entries[obj["key"]] = obj
        except FileNotFoundError:
            pass
        return list(entries.values())


def stop_entry(entry):
    if "stop" in entry:
        subprocess.run(entry["stop"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, timeout=150, check=False)
        return
    pid = entry["pid"]
    try:
        if os.readlink(f"/proc/{pid}/cwd") != os.path.realpath(entry["cwd"]):
            return  # the pid was reused
        os.killpg(pid, signal.SIGTERM)
        for _ in range(100):
            time.sleep(0.1)
            os.killpg(pid, 0)
        os.killpg(pid, signal.SIGKILL)
    except (OSError, ProcessLookupError):
        pass


def reap(journal):
    for entry in reversed(journal.live()):  # newest first: clients before the server they joined
        try:
            stop_entry(entry)
        except (OSError, subprocess.SubprocessError) as e:
            sys.stderr.write(f"reaper: {entry['key']}: {e}\n")
        journal.drop(entry["key"])


def watch(runner_pid, born, group, journal):
    """`born` is the runner's start time, read by the runner; `group` its process group when it leads one (else 0): helper
    scripts it was running (`real-client.sh start`) may still start a process, so the reaper waits for them first."""
    while _start_time(runner_pid) == born:
        time.sleep(1)
    # a runner that shares its group with a harness cannot be waited for by group: allow `real-client.sh start` its 10 s for Xvfb
    for _ in range(30 if group else 15):
        try:
            if group:
                os.killpg(group, 0)
        except OSError:
            break
        time.sleep(1)
    reap(journal)
    for path in (journal.path, journal.path + ".log"):
        try:
            if path == journal.path or os.path.getsize(path) == 0:
                os.unlink(path)
        except OSError:
            pass


def spawn(journal_path, scripts_dir):
    """Starts the reaper for this process in a session of its own (a harness killing the runner's group leaves it alive)."""
    log = open(journal_path + ".log", "ab")
    env = dict(os.environ, PYTHONPATH=scripts_dir + (os.pathsep + os.environ["PYTHONPATH"] if os.environ.get("PYTHONPATH") else ""))
    me = os.getpid()
    group = me if os.getpgid(0) == me else 0
    subprocess.Popen([sys.executable, "-m", "client_e2e.reaper", str(me), _start_time(me), str(group), journal_path], stdin=subprocess.DEVNULL,
                     stdout=log, stderr=log, env=env, start_new_session=True, close_fds=True)
    log.close()
    return Journal(journal_path)


if __name__ == "__main__":
    watch(int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), Journal(sys.argv[4]))
