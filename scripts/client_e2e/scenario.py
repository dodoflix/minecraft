"""Scenario files (`tests/e2e/client/**/*.cscn`): one command per line, `#` starts a comment.

Server setup (applied before the server starts, the same for every side):
    server KEY=VALUE...           server.properties entries (`seed=N` is `level-seed`)
    file PATH SRC                 copy SRC (relative to the scenario file) to PATH in the server directory
    datapack DIR                  copy DIR (relative to the scenario file) to world/datapacks/<name>
    serve /PATH SRC               answer GET /PATH over HTTP on 127.0.0.1 with the file SRC (a directory is served as a zip)
Placeholders in `server` values, `console` text and `ctl` JSON: `{port}` (the server's port), `{http}` (base URL of the
`serve` entries), `{sha1:/PATH}` (the sha1 of a served entry).
Comparison settings:
    tolerate GLOB N               numeric snapshot values at paths matching GLOB (dotted, `*` wildcard) may differ by N
    unordered                     compare the entities of a snapshot sorted by type and position, not in creation order
    ignore GLOB                   snapshot paths matching GLOB are not compared (for values vanilla itself varies run to run)
                                  `entities[TYPE]` leaves the entities of that type out instead
Steps, run in order after the server is up. A step that talks to a client may start with `@NAME ` to address the client
NAME (a second JVM, started by its own `client join`/`client title`, named like its log and snapshot files); without
a prefix the step addresses the first client. Snapshots, shots and chat logs of NAME are labelled `NAME.LABEL`.
    console CMD...                server console command (the next step starts once the server has run it)
    client join [--user U] [--option k:v]...   start the client and join (quick play); options go to options.txt
    client title [--user U] [--option k:v]...  start the client on the title screen with this side's server in its server list
    client rejoin                 reconnect through the control channel
    client disconnect             leave the world (control channel)
    wait_screen NAME|null [S]     wait for the screen class; `null` also needs a level (the world is visible)
    wait_state EXPR [S]           wait until EXPR holds on the latest client state (see below)
    wait_log REGEX [S]            wait for a line in the client log since the last `client join`/`rejoin` (quote a REGEX with spaces)
    wait_server REGEX [S]         wait for a line in the server log since the last `client join`/`rejoin` (or the start)
    sleep S                       wait S seconds of wall time (lets packets of a frozen server arrive before a snapshot)
    ctl JSON                      control channel command (tests/e2e/client/PROTOCOL.md)
    snapshot LABEL                write the state snapshot into the artifacts
    shot LABEL [--crop X,Y,W,H]   save a screenshot (a frame with the crop applied is compared)
    chat_log LABEL                save the `[CHAT]` lines of the client log so far
    expect disconnect REGEX [S]   wait for a disconnect screen whose text matches REGEX
Waits default to 60 s and never sleep for a fixed time. EXPR is a Python-style boolean over the fields of the client's
`[E2E]` state (`screen`, `screen_title`, `in_level`, `joined_ever`, `disconnect_reason`, `game_time`, `chunks`,
`packets_in`, `packets_out`) and `ticks_in_level` (client game time since the first state in a level after the join);
operators `and or not == != < <= > >= + - * / // %`, literals, parentheses. A field that is null or missing makes
a comparison false.
"""
import ast
import json
import operator
import os
import re
import shlex

DEFAULT_WAIT = 60.0

_BIN = {
    ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv,
    ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod,
}
_CMP = {
    ast.Eq: operator.eq, ast.NotEq: operator.ne, ast.Lt: operator.lt, ast.LtE: operator.le,
    ast.Gt: operator.gt, ast.GtE: operator.ge,
}


class ScenarioError(Exception):
    pass


def compile_expr(text):
    """Checks `text` against the allowed grammar and returns a function state-dict -> value."""
    try:
        tree = ast.parse(text.strip(), mode="eval").body
    except SyntaxError as e:
        raise ValueError(f"bad expression {text!r}: {e.msg}") from None

    def build(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float, str, bool, type(None))):
            return lambda s: node.value
        if isinstance(node, ast.Name):
            return lambda s: s.get(node.id)
        if isinstance(node, ast.BoolOp):
            parts = [build(v) for v in node.values]
            if isinstance(node.op, ast.And):
                return lambda s: all(p(s) for p in parts)
            return lambda s: any(p(s) for p in parts)
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.Not, ast.USub)):
            inner = build(node.operand)
            if isinstance(node.op, ast.Not):
                return lambda s: not inner(s)
            return lambda s: None if inner(s) is None else -inner(s)
        if isinstance(node, ast.BinOp) and type(node.op) in _BIN:
            left, right, op = build(node.left), build(node.right), _BIN[type(node.op)]
            return lambda s: None if left(s) is None or right(s) is None else op(left(s), right(s))
        if isinstance(node, ast.Compare) and all(type(o) in _CMP for o in node.ops):
            chain = [build(node.left)] + [build(c) for c in node.comparators]
            ops = [_CMP[type(o)] for o in node.ops]

            def compare(s):
                values = [c(s) for c in chain]
                for op, a, b in zip(ops, values, values[1:]):
                    if a is None or b is None:
                        if op in (operator.eq, operator.ne):
                            if not op(a, b):
                                return False
                            continue
                        return False
                    if not op(a, b):
                        return False
                return True

            return compare
        raise ValueError(f"expression {text!r}: {type(node).__name__} is not allowed")

    return build(tree)


class Scenario:
    def __init__(self, path):
        self.path = path
        self.props = {}  # server.properties entries, in order
        self.files = []  # (destination, source path)
        self.datapacks = []  # source directories
        self.serves = []  # (URL path without the slash, source file or directory)
        self.tolerances = []  # (glob, tolerance)
        self.ignores = []  # globs
        self.unordered = False
        self.steps = []  # (line number, op, args dict)


def _need(cond, line_no, message):
    if not cond:
        raise ScenarioError(f"line {line_no}: {message}")


def _timeout(tokens, line_no, index):
    if len(tokens) <= index:
        return DEFAULT_WAIT
    try:
        return float(tokens[index])
    except ValueError:
        raise ScenarioError(f"line {line_no}: {tokens[index]!r} is not a number of seconds") from None


def _tag(sc, before, who, line_no, op):
    """Records the `@NAME` of a line on the step it produced; only steps that address a client take one."""
    if who is None:
        return
    _need(len(sc.steps) > before and op not in ("console", "wait_server"), line_no, f"@{who} does not apply to {op}")
    sc.steps[-1][2]["client"] = who


def parse(text, path=""):
    """Parses scenario text; `path` anchors relative `file`/`datapack` sources. Raises ScenarioError with the line number."""
    base = os.path.dirname(os.path.abspath(path)) if path else os.getcwd()
    sc = Scenario(path)
    for line_no, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        who = None
        m = re.match(r"@([A-Za-z0-9_]+)\s+(\S.*)$", line)
        if m:
            who, line = m.groups()
        before = len(sc.steps)
        op, _, rest = line.partition(" ")
        rest = rest.strip()
        if op in ("console", "ctl"):  # the rest of the line is taken verbatim
            _need(rest, line_no, f"{op} needs an argument")
            if op == "ctl":
                try:
                    obj = json.loads(rest)
                except ValueError as e:
                    raise ScenarioError(f"line {line_no}: ctl needs a JSON object: {e}") from None
                _need(isinstance(obj, dict) and "cmd" in obj, line_no, 'ctl needs a JSON object with a "cmd"')
                sc.steps.append((line_no, "ctl", {"json": obj}))
            else:
                sc.steps.append((line_no, "console", {"text": rest}))
            _tag(sc, before, who, line_no, op)
            continue
        try:
            tokens = rest.split() if op == "wait_state" else shlex.split(rest)
        except ValueError as e:
            raise ScenarioError(f"line {line_no}: {e}") from None
        if op == "server":
            _need(tokens, line_no, "server needs KEY=VALUE")
            for t in tokens:
                key, eq, value = t.partition("=")
                _need(eq and key, line_no, f"{t!r} is not KEY=VALUE")
                sc.props["level-seed" if key == "seed" else key] = value
        elif op == "file":
            _need(len(tokens) == 2, line_no, "file needs PATH SRC")
            _need(not tokens[0].startswith("/") and ".." not in tokens[0].split("/"), line_no, "file PATH must stay inside the server directory")
            sc.files.append((tokens[0], os.path.join(base, tokens[1])))
        elif op == "datapack":
            _need(len(tokens) == 1, line_no, "datapack needs DIR")
            sc.datapacks.append(os.path.join(base, tokens[0]))
        elif op == "serve":
            _need(len(tokens) == 2 and tokens[0].startswith("/"), line_no, "serve needs /PATH SRC")
            sc.serves.append((tokens[0][1:], os.path.join(base, tokens[1])))
        elif op == "tolerate":
            _need(len(tokens) == 2, line_no, "tolerate needs GLOB N")
            try:
                sc.tolerances.append((tokens[0], float(tokens[1])))
            except ValueError:
                raise ScenarioError(f"line {line_no}: {tokens[1]!r} is not a number") from None
        elif op == "unordered":
            _need(not tokens, line_no, "unordered takes no arguments")
            sc.unordered = True
        elif op == "ignore":
            _need(len(tokens) == 1, line_no, "ignore needs GLOB")
            sc.ignores.append(tokens[0])
        elif op == "client":
            _need(tokens and tokens[0] in ("join", "title", "rejoin", "disconnect"), line_no, "client needs join, title, rejoin or disconnect")
            args = {"action": tokens[0], "user": None, "options": []}
            it = iter(tokens[1:])
            for t in it:
                _need(tokens[0] in ("join", "title") and t in ("--user", "--option"), line_no, f"unexpected {t!r}")
                value = next(it, None)
                _need(value is not None, line_no, f"{t} needs a value")
                if t == "--user":
                    args["user"] = value
                else:
                    _need(":" in value, line_no, "--option wants key:value")
                    args["options"].append(value)
            sc.steps.append((line_no, "client", args))
        elif op == "wait_screen":
            _need(1 <= len(tokens) <= 2, line_no, "wait_screen needs NAME|null [S]")
            sc.steps.append((line_no, "wait_screen", {"screen": None if tokens[0] == "null" else tokens[0], "timeout": _timeout(tokens, line_no, 1)}))
        elif op == "wait_state":
            # the optional last token is the timeout when it is a bare number
            timeout, expr = DEFAULT_WAIT, rest
            m = re.match(r"^(.*\S)\s+(\d+(?:\.\d+)?)$", rest)
            if m and not re.search(r"(==|!=|<=|>=|<|>|[-+*/%]|and|or|not)\s*$", m.group(1)):
                expr, timeout = m.group(1), float(m.group(2))
            _need(expr, line_no, "wait_state needs EXPR")
            try:
                compile_expr(expr)
            except ValueError as e:
                raise ScenarioError(f"line {line_no}: {e}") from None
            sc.steps.append((line_no, "wait_state", {"expr": expr, "timeout": timeout}))
        elif op == "sleep":
            _need(len(tokens) == 1, line_no, "sleep needs S")
            sc.steps.append((line_no, "sleep", {"seconds": _timeout(tokens, line_no, 0)}))
        elif op in ("wait_log", "wait_server"):
            _need(1 <= len(tokens) <= 2, line_no, f"{op} needs REGEX [S]")
            try:
                re.compile(tokens[0])
            except re.error as e:
                raise ScenarioError(f"line {line_no}: bad regex: {e}") from None
            sc.steps.append((line_no, op, {"regex": tokens[0], "timeout": _timeout(tokens, line_no, 1)}))
        elif op == "snapshot" or op == "chat_log":
            _need(len(tokens) == 1 and re.fullmatch(r"[A-Za-z0-9_.-]+", tokens[0]), line_no, f"{op} needs one LABEL of [A-Za-z0-9_.-]")
            sc.steps.append((line_no, op, {"label": tokens[0]}))
        elif op == "shot":
            _need(tokens and re.fullmatch(r"[A-Za-z0-9_.-]+", tokens[0]), line_no, "shot needs LABEL [--crop X,Y,W,H]")
            crop = None
            if len(tokens) > 1:
                _need(len(tokens) == 3 and tokens[1] == "--crop" and re.fullmatch(r"\d+,\d+,\d+,\d+", tokens[2]), line_no, "shot wants --crop X,Y,W,H")
                crop = tuple(int(v) for v in tokens[2].split(","))
            sc.steps.append((line_no, "shot", {"label": tokens[0], "crop": crop}))
        elif op == "expect":
            _need(tokens and tokens[0] == "disconnect" and len(tokens) >= 2, line_no, "expect needs: disconnect REGEX [S]")
            try:
                re.compile(tokens[1])
            except re.error as e:
                raise ScenarioError(f"line {line_no}: bad regex: {e}") from None
            sc.steps.append((line_no, "expect_disconnect", {"regex": tokens[1], "timeout": _timeout(tokens, line_no, 2)}))
        else:
            raise ScenarioError(f"line {line_no}: unknown command {op!r}")
        _tag(sc, before, who, line_no, op)
    return sc


def load(path):
    with open(path, encoding="utf-8") as f:
        return parse(f.read(), path)
