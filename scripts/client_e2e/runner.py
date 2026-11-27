"""Runs one scenario (tests/e2e/client/*.cscn) with the real client against each side in turn and compares the sides.

    python3 -m client_e2e.runner SCENARIO [--sides vanilla,ours] [--out DIR] [--seed N] [--display xvfb|gamescope]
                                 [--known FILE] [--allow FILE] [--server-allow FILE]

Use it through scripts/client-e2e.sh. A side is `vanilla` (ref/server.jar), `ours` (target/debug/mc-server) or either
with property overrides, `vanilla:seed=99,view-distance=6`. Sides run one after the other, never together, each with
its own mock of the Mojang services (mc-testkit `mock-mojang`), the same world setup and the same client script.
Artifacts per side in OUT/LABEL: side.json, client.log, latest.log, server.log, mojang.jsonl, snapshots/, shots/, chat/,
reports/. OUT/report.json and OUT/diff.txt hold the comparison of the first two sides (the first is the baseline).
Exit 0 iff there is no difference that tests/e2e/client/known-differences.txt does not list.
"""
import hashlib
import http.server
import io
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import zipfile

from client_e2e import compare, reaper, scenario, shotdiff

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCRIPTS = os.path.join(ROOT, "scripts")
KNOWN = os.path.join(ROOT, "tests", "e2e", "client", "known-differences.txt")
ALLOW = os.path.join(ROOT, "tests", "e2e", "client", "client-log-noise.txt")
SERVER_ALLOW = os.path.join(ROOT, "tests", "e2e", "client", "server-log-noise.txt")
USER = "Steve"
SCREEN_MIN_MS = 60  # a screen shown for less than this (one client tick is 50 ms) is not part of the compared screen sequence
SIDE_TIMEOUT = int(os.environ.get("MC_E2E_SIDE_TIMEOUT", "1800"))  # seconds one side may run, clients included (a client may queue 900 s for a JVM slot)

# One world setup for every side; a scenario's `server` lines and a side's overrides come after it.
DEFAULT_PROPS = {
    "level-type": "minecraft:flat", "difficulty": "normal", "gamemode": "survival", "force-gamemode": "true",
    "generate-structures": "false",
    "enforce-secure-profile": "false", "spawn-protection": "0", "view-distance": "4", "simulation-distance": "4",
    "online-mode": "false", "white-list": "false", "max-tick-time": "-1", "motd": "e2e",
    "pause-when-empty-seconds": "-1",  # a paused server would not run console commands before the client joins
}

# Run on every side before the scenario: a join must not depend on a random spawn offset, the clock or the weather.
SETUP_COMMANDS = (
    "gamerule respawn_radius 0", "gamerule advance_time false", "gamerule advance_weather false",
    "gamerule spawn_mobs false", "time set 6000", "weather clear",
)


class StepError(Exception):
    pass


# Journal of the processes this run started (client_e2e.reaper): set by main(); a runner that dies without its cleanup (SIGKILL)
# still has them stopped.
GUARD = None


def guard_add(key, **entry):
    if GUARD is not None:
        GUARD.add(key, **entry)


def guard_drop(key):
    if GUARD is not None:
        GUARD.drop(key)


def run_commands(server, commands):
    """Sends console commands and returns once the server has run them: `list` answers after everything before it."""
    marker = re.compile(r"players online", re.M)
    before = len(marker.findall(server.log_text()))
    for command in commands:
        server.cmd(command)
    server.cmd("list")
    deadline = time.time() + 60
    while len(marker.findall(server.log_text())) <= before:
        if time.time() > deadline:
            raise StepError("the server did not answer the setup commands within 60s")
        time.sleep(0.2)


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def sh(*args, env=None, timeout=300, check=True):
    """Runs a command, returns its output; a failure raises StepError with the output.

    Output goes to a file, not a pipe: scripts that start a daemon (vanilla-server.sh) leave it holding their stdout, and a pipe
    would not reach EOF until the daemon exits."""
    with tempfile.TemporaryFile("w+") as out:
        try:
            done = subprocess.run(args, stdout=out, stderr=subprocess.STDOUT, env=env, timeout=timeout, cwd=ROOT, stdin=subprocess.DEVNULL)
        except subprocess.TimeoutExpired:
            raise StepError(f"{os.path.basename(args[0])} {args[1] if len(args) > 1 else ''} timed out after {timeout}s") from None
        out.seek(0)
        text = out.read()
    if check and done.returncode:
        raise StepError(f"{os.path.basename(args[0])} {' '.join(args[1:3])} failed ({done.returncode}): {text.strip()[-600:]}")
    return text


def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except FileNotFoundError:
        return ""

# ---- files served over HTTP -------------------------------------------------------------------------------------------

def blob(src):
    """The bytes a `serve` entry answers with: a file as it is, a directory as a zip with fixed timestamps (equal sha1 on every side)."""
    if not os.path.isdir(src):
        return read_bytes(src)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for dirpath, dirs, names in os.walk(src):
            dirs.sort()
            for n in sorted(names):
                full = os.path.join(dirpath, n)
                info = zipfile.ZipInfo(os.path.relpath(full, src).replace(os.sep, "/"), date_time=(2020, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                z.writestr(info, read_bytes(full))
    return buf.getvalue()


def read_bytes(path):
    try:
        with open(path, "rb") as f:
            return f.read()
    except OSError as e:
        raise StepError(f"serve source {path}: {e.strerror}") from None


class Files:
    """The scenario's `serve PATH SRC` entries on 127.0.0.1; every other path answers 404. `.base`/`.sha1(PATH)` feed `expand`."""

    def __init__(self, serves):
        self.blobs = {"/" + p: blob(src) for p, src in serves}
        blobs = self.blobs

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                body = blobs.get(self.path.split("?")[0])
                self.send_response(200 if body is not None else 404)
                self.send_header("Content-Length", str(len(body or b"")))
                self.end_headers()
                self.wfile.write(body or b"")

            def log_message(self, *args):
                pass

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.base = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def sha1(self, path):
        try:
            return hashlib.sha1(self.blobs[path]).hexdigest()
        except KeyError:
            raise StepError(f"{{sha1:{path}}}: nothing is served at {path}") from None

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()


def expand(text, port, files):
    """Fills `{port}` (the server's), `{http}` (the base URL of `serve`) and `{sha1:/PATH}` (the sha1 of a served file)."""
    return re.sub(
        r"\{(port|http|sha1:[^}]*)\}",
        lambda m: str(port) if m.group(1) == "port" else files.base if m.group(1) == "http" else files.sha1(m.group(1)[5:]),
        text,
    )



# ---- the services mock ----------------------------------------------------------------------------------------------

class Mojang:
    def __init__(self, log):
        binary = os.path.abspath(os.environ.get("MC_MOCK_MOJANG_BIN", os.path.join(ROOT, "target", "debug", "mock-mojang")))
        if not os.access(binary, os.X_OK):
            raise StepError(f"no mock-mojang binary at {binary}: cargo build --locked -p mc-testkit --bin mock-mojang")
        self.proc = subprocess.Popen([binary, "--log", log], stdout=subprocess.PIPE, text=True, start_new_session=True)
        self.key = f"mojang:{self.proc.pid}"
        guard_add(self.key, pid=self.proc.pid, cwd=os.getcwd())
        self.url = (self.proc.stdout.readline() or "").strip()
        if not self.url.startswith("http://"):
            self.stop()
            raise StepError("mock-mojang printed no discovery URL")

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.proc.stdout.close()
        guard_drop(self.key)


# ---- servers ----------------------------------------------------------------------------------------------------------

def prepare_dir(sc, server_dir):
    """Puts the scenario's files and data packs into the server directory."""
    for dest, src in sc.files:
        if not os.path.exists(src):
            raise StepError(f"file {src} does not exist")
        target = os.path.join(server_dir, dest)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copytree(src, target) if os.path.isdir(src) else shutil.copyfile(src, target)
    for src in sc.datapacks:
        if not os.path.isdir(src):
            raise StepError(f"datapack {src} is not a directory")
        shutil.copytree(src, os.path.join(server_dir, "world", "datapacks", os.path.basename(src.rstrip("/"))))


class VanillaServer:
    def __init__(self, name, props, port, mojang_url, sc):
        self.name, self.props, self.port, self.mojang_url, self.sc = name, props, port, mojang_url, sc
        self.dir = os.path.join(ROOT, "run", "servers", name)

    def start(self):
        shutil.rmtree(self.dir, ignore_errors=True)
        os.makedirs(self.dir)
        prepare_dir(self.sc, self.dir)
        env = dict(os.environ, VANILLA_JVM_OPTS=f"-Dminecraft.api.discovery.host={self.mojang_url}")
        extras = [f"{k}={v}" for k, v in self.props.items()]
        guard_add(f"server:{self.name}", stop=[os.path.join(SCRIPTS, "vanilla-server.sh"), "stop", self.name])  # before the start: it may time out half way
        sh(os.path.join(SCRIPTS, "vanilla-server.sh"), "start", self.name, "--port", str(self.port), "--xmx", "1G", *extras, env=env, timeout=400)

    def cmd(self, text):
        sh(os.path.join(SCRIPTS, "vanilla-server.sh"), "cmd", self.name, text)

    def log_text(self):
        return read(os.path.join(self.dir, "logs", "latest.log"))

    def stop(self):
        sh(os.path.join(SCRIPTS, "vanilla-server.sh"), "stop", self.name, check=False, timeout=120)
        guard_drop(f"server:{self.name}")


class OursServer:
    def __init__(self, dir_, props, port, mojang_url, sc):
        self.dir, self.props, self.port, self.mojang_url, self.sc = dir_, props, port, mojang_url, sc
        self.proc = None
        self.log_path = os.path.join(dir_, "out.log")

    def start(self):
        binary = os.path.abspath(os.environ.get("MC_SERVER_BIN", os.path.join(ROOT, "target", "debug", "mc-server")))
        if not os.access(binary, os.X_OK):
            raise StepError(f"no mc-server binary at {binary}: cargo build --locked -p mc-server")
        os.makedirs(self.dir, exist_ok=True)
        prepare_dir(self.sc, self.dir)
        # eula.txt is written by scripts/lib.sh only (it checks the opt-in)
        sh("bash", "-c", '. "$1/scripts/lib.sh"; write_eula "$2"', "_", ROOT, self.dir)
        props = {**self.props, "server-ip": "127.0.0.1", "server-port": str(self.port)}
        with open(os.path.join(self.dir, "server.properties"), "w", encoding="utf-8") as f:
            f.writelines(f"{k}={v}\n" for k, v in props.items())
        log = open(self.log_path, "wb")
        env = dict(os.environ, MINECRAFT_API_DISCOVERY_HOST=self.mojang_url)
        self.proc = subprocess.Popen([binary, "nogui"], cwd=self.dir, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, env=env, start_new_session=True)
        log.close()
        guard_add(f"server:{self.dir}", pid=self.proc.pid, cwd=self.dir)
        deadline = time.time() + 240
        while time.time() < deadline:
            if "Done (" in self.log_text():
                return
            if self.proc.poll() is not None:
                break
            time.sleep(0.5)
        raise StepError("our server did not start:\n" + self.log_text()[-800:])

    def cmd(self, text):
        try:
            self.proc.stdin.write((text + "\n").encode())
            self.proc.stdin.flush()
        except OSError as e:
            raise StepError(f"our server is not running: {e}") from e

    def log_text(self):
        return read(self.log_path)

    def stop(self):
        if self.proc is None:
            return
        if self.proc.poll() is None:
            try:
                self.cmd("stop")
                self.proc.stdin.close()
                self.proc.wait(60)
            except (subprocess.TimeoutExpired, StepError, OSError):
                pass
            if self.proc.poll() is None:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait()
        guard_drop(f"server:{self.dir}")


# ---- the client -------------------------------------------------------------------------------------------------------

class Client:
    """One real-client process of a side and the state read from its log."""

    def __init__(self, name, display, tag=None):
        self.name, self.display, self.tag = name, display, tag
        self.client_from = self.disc_from = 0  # where wait_log and expect_disconnect start reading: set by join and rejoin
        self.dir = os.path.join(ROOT, "run", "clients", name)
        self.offset = 0
        self.state = {}
        self.base_ticks = None
        self.seen, self.pending, self.disconnects = [], None, []  # screens: see the property
        self.user = None
        self.last_screen = self.last_reason = None  # of the state lines read so far, kept when a rejoin clears `state`

    def real(self, *args, **kw):
        return sh(os.path.join(SCRIPTS, "real-client.sh"), *args, **kw)

    def start(self, user, addr, mojang_url, options, servers=()):
        """`addr` None = the title screen; `servers` (`NAME=HOST:PORT`) go into the client's server list."""
        self.user = user
        self.offset, self.state, self.base_ticks = 0, {}, None
        self.last_screen = self.last_reason = None
        display = ["--display", self.display] if self.display else []
        guard_add(f"client:{self.name}", stop=[os.path.join(SCRIPTS, "real-client.sh"), "stop", self.name])
        self.real("start", self.name, user, addr or "none", "--mojang", mojang_url, *display, *[x for o in options for x in ("--option", o)],
                  *[x for s in servers for x in ("--server", s)], timeout=180)
        # the JVM may queue for a machine-wide client slot: a step's own timeout starts once the client prints its first state
        self.wait(lambda: bool(self.state), 900, "the client to start")

    def alive(self):
        try:
            with open(os.path.join(self.dir, "pid"), encoding="utf-8") as f:
                os.kill(int(f.read()), 0)
            return True
        except (OSError, ValueError):
            return False

    @property
    def screens(self):
        """The screens the client showed, in order: those that stayed at least SCREEN_MIN_MS, and the current one."""
        if self.pending and (not self.seen or self.seen[-1] != self.pending[0]):
            return self.seen + [self.pending[0]]
        return list(self.seen)

    def settle(self, now):
        """Closes the current screen at time `now` (ms): a screen that stayed less than SCREEN_MIN_MS is dropped.
        One that lives for a frame or a tick (the parent of a prompt after it disconnects, a progress screen while
        connecting) is seen by the poll or not depending on timing, and no player could read it either."""
        if not self.pending:
            return
        name, since = self.pending
        if (now is None or since is None or now - since >= SCREEN_MIN_MS) and (not self.seen or self.seen[-1] != name):
            self.seen.append(name)

    def pump(self):
        """Reads the new part of the client log and applies the `[E2E]` state lines."""
        try:
            with open(os.path.join(self.dir, "client.out"), "rb") as f:
                f.seek(self.offset)
                chunk = f.read()
        except FileNotFoundError:
            return
        end = chunk.rfind(b"\n") + 1  # a half-written line waits for the next call
        self.offset += end
        for line in chunk[:end].decode("utf-8", "replace").splitlines():
            if "[E2E] {" not in line:
                continue
            try:
                state = json.loads(line.split("[E2E] ", 1)[1])
            except ValueError:
                continue
            # the first line, printed while the client starts, may or may not come before its first screen
            screen = state.get("screen")
            if screen != self.last_screen and (self.seen or self.pending or screen):
                self.settle(state.get("t"))
                self.pending = (screen, state.get("t"))
            self.last_screen = screen
            reason = state.get("disconnect_reason")
            if reason and reason != self.last_reason:
                self.disconnects.append(reason)
            self.last_reason = reason
            if state.get("in_level") and self.base_ticks is None and isinstance(state.get("game_time"), (int, float)) and state["game_time"] > 0:
                self.base_ticks = state["game_time"]
            self.state = state
            self.state["ticks_in_level"] = None if self.base_ticks is None else state.get("game_time", 0) - self.base_ticks

    def wait(self, predicate, timeout, what):
        deadline = time.time() + timeout
        while True:
            self.pump()
            if predicate():
                return
            if not self.alive():
                self.pump()
                if predicate():
                    return
                raise StepError(f"client exited while waiting for {what}")
            if time.time() > deadline:
                raise StepError(f"timeout after {timeout:g}s waiting for {what}; last state {json.dumps(self.state)}")
            time.sleep(0.2)

    def ctl(self, obj, timeout=30):
        return sh(os.path.join(SCRIPTS, "client-ctl.sh"), self.name, json.dumps(obj), "--timeout", str(timeout), timeout=timeout + 15)

    def log_text(self):
        return read(os.path.join(self.dir, "client.out"))

    def stop(self):
        self.real("stop", self.name, check=False, timeout=60)
        guard_drop(f"client:{self.name}")


# ---- one side ---------------------------------------------------------------------------------------------------------

def run_steps(sc, ctx):
    server, out, clients = ctx["server"], ctx["dir"], ctx["clients"]
    # wait_log and wait_server read what was logged since the last join or rejoin: a line that repeats per connection
    # (`joined the game`) must not satisfy the wait of the next one
    server_from = 0
    for line_no, op, a in sc.steps:
        try:
            tag = a.get("client")
            client = clients.get(tag)
            if client is None:
                client = clients[tag] = Client(f"{ctx['name']}-{tag}", ctx["display"], tag)
            prefix = "" if tag is None else f"{tag}."  # labels of a second client's snapshots, shots and chat logs
            if op == "console":
                server.cmd(expand(a["text"], ctx["port"], ctx["files"]))
            elif op == "client":
                address = f"127.0.0.1:{ctx['port']}"
                if a["action"] in ("join", "title"):
                    server_from = len(server.log_text())
                    user = a["user"] or (USER if tag is None else f"{USER}_{tag}")
                    if a["action"] == "join":
                        client.start(user, address, ctx["mojang"].url, a["options"])
                    else:
                        client.start(user, None, ctx["mojang"].url, a["options"], servers=[f"e2e={address}"])
                    client.client_from, client.disc_from = 0, len(client.disconnects)
                elif a["action"] == "rejoin":
                    server_from, client.client_from, client.disc_from = len(server.log_text()), len(client.log_text()), len(client.disconnects)
                    client.ctl({"cmd": "connect", "address": address})
                    # the state of the old connection stays until the client prints a state line of the new one
                    client.pump()
                    client.state, client.base_ticks = {}, None
                else:
                    client.ctl({"cmd": "disconnect"})
            elif op == "wait_screen":
                want = a["screen"]
                client.wait(lambda: client.state.get("screen") == want and (want is not None or client.state.get("in_level")), a["timeout"], f"screen {want}")
            elif op == "wait_state":
                fn = scenario.compile_expr(a["expr"])
                client.wait(lambda: bool(fn(client.state)), a["timeout"], a["expr"])
            elif op == "wait_log":
                client.wait(lambda: re.search(a["regex"], client.log_text()[client.client_from:], re.M), a["timeout"], f"client log /{a['regex']}/")
            elif op == "wait_server":
                deadline = time.time() + a["timeout"]
                while not re.search(a["regex"], server.log_text()[server_from:], re.M):
                    if time.time() > deadline:
                        raise StepError(f"timeout after {a['timeout']:g}s waiting for server log /{a['regex']}/")
                    time.sleep(0.2)
            elif op == "ctl":
                client.ctl(json.loads(expand(json.dumps(a["json"]), ctx["port"], ctx["files"])))
            elif op == "snapshot":
                path = os.path.join(out, "snapshots", prefix + a["label"] + ".json")
                os.makedirs(os.path.dirname(path), exist_ok=True)
                client.ctl({"cmd": "snapshot", "path": path})
                if not os.path.exists(path):
                    raise StepError(f"the client reported the snapshot but {path} does not exist")
            elif op == "shot":
                path = os.path.join(out, "shots", prefix + a["label"] + ".png")
                os.makedirs(os.path.dirname(path), exist_ok=True)
                client.ctl({"cmd": "screenshot", "path": path})
                if a["crop"]:
                    shotdiff.crop_file(path, a["crop"])
            elif op == "chat_log":
                path = os.path.join(out, "chat", prefix + a["label"] + ".txt")
                os.makedirs(os.path.dirname(path), exist_ok=True)
                client.pump()
                with open(path, "w", encoding="utf-8") as f:
                    f.writelines(m.group(1) + "\n" for m in re.finditer(r"\[CHAT\] (.*)$", client.log_text(), re.M))
            elif op == "expect_disconnect":
                client.wait(lambda: any(re.search(a["regex"], d) for d in client.disconnects[client.disc_from:]), a["timeout"], f"disconnect /{a['regex']}/")
        except (StepError, ValueError, TypeError, ZeroDivisionError, OSError) as e:
            raise StepError(f"line {line_no} ({op}{'' if tag is None else ' @' + tag}): {e}") from None


def save_client(client, out):
    if client.user is None:  # never started: run/clients/NAME still holds the files of an earlier run
        return
    suffix = "" if client.tag is None else f"-{client.tag}"
    client.pump()
    client.stop()
    log = os.path.join(client.dir, "client.out")
    if os.path.exists(log):
        shutil.copyfile(log, os.path.join(out, f"client{suffix}.log"))
    for game_log in ("latest.log", os.path.join("logs", "latest.log")):
        p = os.path.join(client.dir, "game", game_log)
        if os.path.exists(p):
            shutil.copyfile(p, os.path.join(out, f"latest{suffix}.log"))
            break
    for sub in ("crash-reports", "debug"):
        p = os.path.join(client.dir, "game", sub)
        if os.path.isdir(p) and os.listdir(p):
            shutil.copytree(p, os.path.join(out, "reports", sub + suffix), dirs_exist_ok=True)


def save_clients(clients, out):
    """Saves every client; each one is stopped even when saving an earlier one raises."""
    pending = list(clients.values())
    try:
        while pending:
            save_client(pending.pop(0), out)
    finally:
        for c in pending:
            try:
                save_client(c, out)
            except (StepError, OSError):
                pass


def save_server(server, out):
    log = server.log_text()
    try:
        server.stop()
    finally:
        with open(os.path.join(out, "server.log"), "w", encoding="utf-8") as f:
            f.write(server.log_text() or log)


def run_side(sc, kind, label, overrides, out, opts):
    """Runs the scenario against one server; returns nothing, writes the artifacts. Step failures are recorded, not raised."""
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    scn = os.path.splitext(os.path.basename(sc.path))[0]
    port = free_port()
    errors, mojang, server, clients, files = [], None, None, {}, None
    def too_long(*_):
        raise StepError(f"the side ran longer than {SIDE_TIMEOUT}s (MC_E2E_SIDE_TIMEOUT)")

    signal.signal(signal.SIGALRM, too_long)
    signal.alarm(SIDE_TIMEOUT)  # a stuck client must not hold the JVM slots: the step error stops everything in `finally`
    try:
        files = Files(sc.serves)
        props = {k: expand(v, port, files) for k, v in {**DEFAULT_PROPS, "level-seed": str(opts["seed"]), **sc.props, **overrides}.items()}
        mojang = Mojang(os.path.join(out, "mojang.jsonl"))
        name = f"e2e-{scn}-{label}"
        if kind == "vanilla":
            server = VanillaServer(name, props, port, mojang.url, sc)
        else:
            server = OursServer(os.path.join(out, "server"), props, port, mojang.url, sc)
        server.start()
        run_commands(server, SETUP_COMMANDS)
        clients[None] = Client(name, opts["display"])
        run_steps(sc, {"clients": clients, "name": name, "display": opts["display"], "server": server, "dir": out, "port": port, "mojang": mojang, "files": files})
    except (StepError, OSError) as e:
        errors.append(str(e))
    finally:
        signal.alarm(0)
        # each stop runs even when an earlier one raises: no server, client or mock is left behind
        try:
            save_clients(clients, out)
        finally:
            try:
                if server is not None:
                    save_server(server, out)
            finally:
                try:
                    if mojang is not None:
                        mojang.stop()
                finally:
                    if files is not None:
                        files.stop()
    client = clients.get(None)
    meta = {
        "kind": kind, "label": label, "errors": errors,
        "screens": client.screens if client else [], "disconnects": client.disconnects if client else [],
        "extra": {t: {"screens": c.screens, "disconnects": c.disconnects} for t, c in clients.items() if t is not None},
    }
    with open(os.path.join(out, "side.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)
    return meta


# ---- command line -----------------------------------------------------------------------------------------------------

def parse_sides(text):
    """`vanilla,ours:view-distance=6` -> [(kind, label, overrides)]; a repeated kind gets a number."""
    sides, seen = [], {}
    for spec in re.split(r",(?=(?:vanilla|ours)\b)", text):
        kind, _, rest = spec.partition(":")
        if kind not in ("vanilla", "ours"):
            raise ValueError(f"side {spec!r}: wants vanilla or ours, optionally with :key=value,key=value")
        overrides = {}
        for item in filter(None, rest.split(",")):
            key, eq, value = item.partition("=")
            if not eq:
                raise ValueError(f"side {spec!r}: {item!r} is not key=value")
            overrides["level-seed" if key == "seed" else key] = value
        seen[kind] = seen.get(kind, 0) + 1
        sides.append((kind, kind if seen[kind] == 1 else f"{kind}{seen[kind]}", overrides))
    return sides


def parse_args(argv):
    opts = {"sides": "vanilla,ours", "out": None, "seed": 12345, "display": None, "known": KNOWN, "allow": ALLOW, "server_allow": SERVER_ALLOW}
    positional, i = [], 0
    names = {"--sides": "sides", "--out": "out", "--seed": "seed", "--display": "display", "--known": "known", "--allow": "allow", "--server-allow": "server_allow"}
    while i < len(argv):
        arg = argv[i]
        if arg in names:
            if i + 1 >= len(argv):
                raise ValueError(f"{arg} needs a value")
            opts[names[arg]] = argv[i + 1]
            i += 2
        elif arg.startswith("--"):
            raise ValueError(f"unknown option {arg}")
        else:
            positional.append(arg)
            i += 1
    if len(positional) != 1:
        raise ValueError("needs exactly one SCENARIO")
    opts["seed"] = int(opts["seed"])
    return positional[0], opts


def single_side_diffs(out, allow, server_allow):
    """Differences of a lone side (no baseline): its errors and log findings."""
    from client_e2e import gate, normalise

    meta = json.loads(read(os.path.join(out, "side.json")) or "{}")
    diffs = compare.error_diffs(meta.get("label"), meta.get("errors", []))
    for tag in [None, *sorted(meta.get("extra", {}))]:
        found, _ = gate.findings(gate.parse(compare.client_log_text(out, tag)), allow)
        diffs += [(f"client-log{'' if tag is None else '@' + tag}:{meta.get('label')}:{f}", f) for f in found]
    diffs += [(f"server-log:{meta.get('label')}:{f}", f) for f in normalise.server_findings(read(os.path.join(out, "server.log"))) if f not in server_allow]
    diffs += [(f"client-report:{meta.get('label')}:{os.path.basename(f)}", f) for f in gate.extra_files([os.path.join(out, "reports")])]
    return diffs


def main(argv):
    from client_e2e import gate

    try:
        path, opts = parse_args(argv)
        sc = scenario.load(path)
        sides = parse_sides(opts["sides"])
    except (ValueError, scenario.ScenarioError, OSError) as e:
        sys.stderr.write(f"client-e2e: {e}\n")
        return 2
    # the `finally` of a side still stops its processes (and so releases the JVM slots)
    global GUARD
    out_root = os.path.join(ROOT, "run", "client-e2e")
    os.makedirs(out_root, exist_ok=True)
    GUARD = reaper.spawn(os.path.join(out_root, f".journal-{os.getpid()}"), SCRIPTS)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    signal.signal(signal.SIGINT, lambda *_: sys.exit(130))
    name = os.path.splitext(os.path.basename(path))[0]
    out = os.path.abspath(opts["out"] or os.path.join(ROOT, "run", "client-e2e", name))
    os.makedirs(out, exist_ok=True)
    allow = gate.load_allow(opts["allow"]) if os.path.exists(opts["allow"]) else []
    server_allow = gate.load_allow(opts["server_allow"]) if os.path.exists(opts["server_allow"]) else []
    for kind, label, overrides in sides:
        sys.stdout.write(f"== {name}: {label}\n")
        sys.stdout.flush()
        meta = run_side(sc, kind, label, overrides, os.path.join(out, label), opts)
        for e in meta["errors"]:
            sys.stdout.write(f"   {label}: {e}\n")
    if len(sides) >= 2:
        diffs = compare.compare_sides(os.path.join(out, sides[0][1]), os.path.join(out, sides[1][1]), sc.tolerances, allow, server_allow=server_allow, ignore=sc.ignores, unordered=sc.unordered)
    else:
        diffs = single_side_diffs(os.path.join(out, sides[0][1]), allow, server_allow)
    unexpected, listed, stale = compare.apply_known(name, diffs, compare.load_known(opts["known"]))
    report = {
        "scenario": name, "sides": [s[1] for s in sides], "ok": not unexpected,
        "differences": [{"key": k, "detail": d} for k, d in unexpected],
        "known": [{"key": k, "detail": d} for k, d in listed], "now_equal": stale,
    }
    with open(os.path.join(out, "report.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)
    lines = [f"{k}\n    {d}" for k, d in unexpected]
    lines += [f"(known) {k}" for k, _ in listed]
    lines += [f"(listed in known-differences.txt but equal now: remove) {k}" for k in stale]
    with open(os.path.join(out, "diff.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + ("\n" if lines else ""))
    verdict = "equal" if not unexpected else f"{len(unexpected)} difference(s)"
    sys.stdout.write("\n".join(lines) + ("\n" if lines else "") + f"{name}: {verdict}  ({out})\n")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
