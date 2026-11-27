"""Scenario grammar, side parsing and the comparer on synthetic artifacts: python3 tests/e2e/client/test_runner.py"""
import hashlib
import json
import urllib.error
import urllib.request
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(HERE))), "scripts"))
from client_e2e import compare, runner, scenario  # noqa: E402

FULL = """
# comment
server seed=7 level-type=minecraft:flat
file config/x.txt data/x.txt
datapack pack
tolerate level.chunks_loaded 5
ignore entities.*
unordered
console op Steve
client join --user Alex --option lang:en_us
wait_screen null 90
wait_screen ConnectScreen
wait_state in_level and ticks_in_level >= 200 30
wait_state chunks > 3
wait_log "Loaded \\d+ advancements" 5
wait_server joined
ctl {"cmd": "chat", "text": "hi there"}
snapshot joined
shot hud --crop 0,0,100,50
chat_log after
client rejoin
client disconnect
expect disconnect "not white" 4
"""


class ScenarioTest(unittest.TestCase):
    def test_a_full_scenario_parses(self):
        sc = scenario.parse(FULL, "/s/a.cscn")
        self.assertEqual(sc.props, {"level-seed": "7", "level-type": "minecraft:flat"})
        self.assertEqual(sc.files, [("config/x.txt", "/s/data/x.txt")])
        self.assertEqual(sc.datapacks, ["/s/pack"])
        self.assertEqual(sc.tolerances, [("level.chunks_loaded", 5.0)])
        self.assertEqual(sc.ignores, ["entities.*"])
        self.assertTrue(sc.unordered)
        steps = {op: a for _, op, a in sc.steps}
        self.assertEqual(steps["client"]["action"], "disconnect")
        self.assertEqual([a for _, op, a in sc.steps if op == "client"][0], {"action": "join", "user": "Alex", "options": ["lang:en_us"]})
        self.assertEqual([a["screen"] for _, op, a in sc.steps if op == "wait_screen"], [None, "ConnectScreen"])
        self.assertEqual([a["timeout"] for _, op, a in sc.steps if op == "wait_screen"], [90.0, 60.0])
        waits = [a for _, op, a in sc.steps if op == "wait_state"]
        self.assertEqual(waits[0], {"expr": "in_level and ticks_in_level >= 200", "timeout": 30.0})
        self.assertEqual(waits[1], {"expr": "chunks > 3", "timeout": 60.0})
        self.assertEqual(steps["wait_log"], {"regex": "Loaded \\d+ advancements", "timeout": 5.0})
        self.assertEqual(steps["ctl"], {"json": {"cmd": "chat", "text": "hi there"}})
        self.assertEqual(steps["shot"], {"label": "hud", "crop": (0, 0, 100, 50)})
        self.assertEqual(steps["expect_disconnect"], {"regex": "not white", "timeout": 4.0})
        self.assertEqual(steps["console"], {"text": "op Steve"})

    def test_errors_name_the_line(self):
        for text, fragment in [
            ("server x", "line 1: 'x' is not KEY=VALUE"),
            ("\n\nbogus 1", "line 3: unknown command 'bogus'"),
            ("ctl {not json}", "line 1: ctl needs a JSON object"),
            ('ctl {"text": "x"}', '"cmd"'),
            ("wait_state ticks >=", "line 1: bad expression"),
            ("wait_state __import__('os')", "not allowed"),
            ("wait_screen", "line 1"),
            ("wait_log (", "bad regex"),
            ("client join --user", "--user needs a value"),
            ("client join --option nocolon", "key:value"),
            ("client flee", "client needs"),
            ("file ../x y", "must stay inside"),
            ("shot a --crop 1,2", "--crop X,Y,W,H"),
            ("snapshot bad/label", "LABEL"),
            ("tolerate x y", "not a number"),
            ("ignore", "ignore needs GLOB"),
            ("unordered now", "takes no arguments"),
        ]:
            with self.assertRaises(scenario.ScenarioError, msg=text) as raised:
                scenario.parse(text)
            self.assertIn(fragment, str(raised.exception), text)

    def test_a_step_can_address_a_second_client(self):
        sc = scenario.parse("client join\n@b client join --user Alex\n@b wait_screen null 5\n@b snapshot x\nconsole say hi\n")
        self.assertEqual([(op, a.get("client")) for _, op, a in sc.steps], [("client", None), ("client", "b"), ("wait_screen", "b"), ("snapshot", "b"), ("console", None)])
        for text in ("@b console say hi", "@b server x=1", "@ client join"):
            with self.assertRaises(scenario.ScenarioError, msg=text):
                scenario.parse(text)

    def test_expressions(self):
        ev = lambda text, **state: scenario.compile_expr(text)(state)  # noqa: E731
        self.assertTrue(ev("in_level and game_time >= 10", in_level=True, game_time=12))
        self.assertFalse(ev("game_time >= 10", game_time=None))  # null makes a comparison false
        self.assertFalse(ev("game_time >= 10"))
        self.assertTrue(ev("screen == None"))  # a missing field equals null
        self.assertTrue(ev("not in_level or chunks // 2 == 3", in_level=True, chunks=7))
        self.assertTrue(ev("0 < chunks <= 10", chunks=4))
        self.assertTrue(ev("screen == 'ConnectScreen'", screen="ConnectScreen"))


class PumpTest(unittest.TestCase):
    def test_a_rejoin_keeps_the_screen_and_disconnect_sequences(self):
        with tempfile.TemporaryDirectory() as tmp:
            client = runner.Client("t", None)
            client.dir = tmp

            def feed(**state):
                with open(os.path.join(tmp, "client.out"), "a", encoding="utf-8") as f:
                    f.write(f"[12:00:00] [e2e-watch/INFO]: [STDOUT]: [E2E] {json.dumps(state)}\n")
                client.pump()

            feed(screen=None, in_level=False)  # printed while the client starts: no screen yet
            feed(screen="ConnectScreen", in_level=False)
            feed(screen=None, in_level=True, game_time=10)
            feed(screen=None, in_level=True, game_time=70)
            self.assertEqual(client.state["ticks_in_level"], 60)
            feed(screen="DisconnectedScreen", in_level=False, disconnect_reason="Kicked")
            feed(screen="DisconnectedScreen", in_level=False, disconnect_reason="Kicked")
            client.state, client.base_ticks = {}, None  # what `client rejoin` does
            feed(screen="DisconnectedScreen", in_level=False, disconnect_reason="Kicked")
            feed(screen="ConnectScreen", in_level=False)
            feed(screen=None, in_level=True, game_time=500)
            self.assertEqual(client.state["ticks_in_level"], 0)
            self.assertEqual(client.screens, ["ConnectScreen", None, "DisconnectedScreen", "ConnectScreen", None])
            self.assertEqual(client.disconnects, ["Kicked"])

    def test_a_screen_shown_for_less_than_a_moment_is_not_in_the_sequence(self):
        with tempfile.TemporaryDirectory() as tmp:
            client = runner.Client("t", None)
            client.dir = tmp

            def feed(t, screen):
                with open(os.path.join(tmp, "client.out"), "a", encoding="utf-8") as f:
                    f.write(f"[12:00:00] [e2e-watch/INFO]: [STDOUT]: [E2E] {json.dumps({'screen': screen, 't': t})}\n")
                client.pump()

            feed(1000, "PackConfirmScreen")
            feed(2000, "ConnectScreen")  # the parent of the prompt: one client tick
            feed(2030, "DisconnectedScreen")
            self.assertEqual(client.screens, ["PackConfirmScreen", "DisconnectedScreen"])
            feed(3000, "ProgressScreen")
            feed(3010, "PackConfirmScreen")  # a blip, then the screen again
            self.assertEqual(client.screens, ["PackConfirmScreen", "DisconnectedScreen", "PackConfirmScreen"])


class ServeTest(unittest.TestCase):
    def test_files_are_served_and_placeholders_filled(self):
        with tempfile.TemporaryDirectory() as tmp:
            pack = os.path.join(tmp, "pack")
            os.makedirs(os.path.join(pack, "assets"))
            for name in ("pack.mcmeta", os.path.join("assets", "a.json")):
                with open(os.path.join(pack, name), "w") as f:
                    f.write("{}")
            self.assertEqual(runner.blob(pack), runner.blob(pack))  # fixed timestamps: the sha1 is stable
            files = runner.Files([("pack.zip", pack)])
            self.addCleanup(files.stop)
            with urllib.request.urlopen(files.base + "/pack.zip") as r:
                body = r.read()
            self.assertEqual(body, runner.blob(pack))
            with self.assertRaises(urllib.error.HTTPError) as missing:
                urllib.request.urlopen(files.base + "/other.zip")
            self.assertEqual(missing.exception.code, 404)
            text = runner.expand("{http}/pack.zip {sha1:/pack.zip} {port} {\"a\": 1}", 25565, files)
            self.assertEqual(text, f"{files.base}/pack.zip {hashlib.sha1(body).hexdigest()} 25565 {{\"a\": 1}}")
            with self.assertRaises(runner.StepError):
                runner.expand("{sha1:/nope}", 1, files)

    def test_serve_parses(self):
        sc = scenario.parse("serve /pack.zip fixtures/pack\nserver resource-pack={http}/pack.zip\n", "/x/y.cscn")
        self.assertEqual(sc.serves, [("pack.zip", "/x/fixtures/pack")])
        self.assertEqual(sc.props["resource-pack"], "{http}/pack.zip")
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse("serve pack.zip x")


class ErrorKeyTest(unittest.TestCase):
    def test_the_key_names_the_step_not_the_message(self):
        errors = ['line 7 (wait_screen): timeout after 60s waiting for screen null; last state {"game_time": 41}', "mock-mojang printed no discovery URL"]
        self.assertEqual(
            [k for k, _ in compare.error_diffs("ours", errors)],
            ["error:ours:line 7 (wait_screen)", "error:ours:mock-mojang printed no discovery URL"],
        )


class SidesTest(unittest.TestCase):
    def test_sides_labels_and_overrides(self):
        self.assertEqual(runner.parse_sides("vanilla,ours"), [("vanilla", "vanilla", {}), ("ours", "ours", {})])
        self.assertEqual(
            runner.parse_sides("vanilla,vanilla:seed=99,view-distance=6"),
            [("vanilla", "vanilla", {}), ("vanilla", "vanilla2", {"level-seed": "99", "view-distance": "6"})],
        )
        with self.assertRaises(ValueError):
            runner.parse_sides("vanila")
        with self.assertRaises(ValueError):
            runner.parse_sides("vanilla:seed")

    def test_arguments(self):
        path, opts = runner.parse_args(["a.cscn", "--sides", "vanilla", "--seed", "3"])
        self.assertEqual((path, opts["sides"], opts["seed"]), ("a.cscn", "vanilla", 3))
        for bad in (["--sides", "x"], ["a", "b"], ["a", "--nope", "1"]):
            with self.assertRaises(ValueError):
                runner.parse_args(bad)


def snapshot(chunks, pig_id, time):
    return {
        "format": 2,
        "level": {"dimension": "minecraft:overworld", "game_time": time, "chunks_loaded": chunks},
        "player": {"name": "Steve", "uuid": "p", "health": 20.0},
        "entities": [
            {"id": pig_id, "uuid": f"random-{pig_id}", "type": "minecraft:pig", "passengers": []},
            {"id": pig_id + 1, "uuid": "p", "type": "minecraft:player", "passengers": [pig_id]},
        ],
        "packets": {"in": {"play:clientbound/login": 1}},
    }


def write_side(root, label, snap, log="[12:00:00] [Render thread/INFO]: Loaded 2 advancements\n", server=""):
    d = os.path.join(root, label)
    os.makedirs(os.path.join(d, "snapshots"))
    with open(os.path.join(d, "snapshots", "joined.json"), "w") as f:
        json.dump(snap, f)
    with open(os.path.join(d, "side.json"), "w") as f:
        json.dump({"label": label, "errors": [], "screens": [None, "ConnectScreen", "LevelLoadingScreen", None], "disconnects": []}, f)
    with open(os.path.join(d, "client.log"), "w") as f:
        f.write(log)
    with open(os.path.join(d, "server.log"), "w") as f:
        f.write(server)
    return d


class CompareTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def sides(self, a, b, server=""):
        return write_side(self.tmp.name, "vanilla", a, server=server), write_side(self.tmp.name, "ours", b, log=self.ours_log)

    ours_log = "[12:00:00] [Render thread/INFO]: Loaded 2 advancements\n"

    def test_entity_ids_uuids_and_game_time_are_not_differences(self):
        a, b = self.sides(snapshot(100, 5, 40), snapshot(100, 90, 777))
        self.assertEqual(compare.compare_sides(a, b), [])

    def test_packet_counters_compare_by_presence_unless_pinned(self):
        a, b = snapshot(1, 1, 1), snapshot(1, 1, 1)
        a["packets"] = {"in": {"login": 1, "tick": 18}}
        b["packets"] = {"in": {"login": 2, "tick": 120, "extra": 1}}
        self.assertEqual([p for p, _, _ in compare.diff_snapshots(a, b)], ["packets.in.extra"])
        pinned = compare.diff_snapshots(a, b, tolerances=[("packets.in.login", 0)])
        self.assertEqual([p for p, _, _ in pinned], ["packets.in.extra", "packets.in.login"])
        self.assertEqual([p for p, _, _ in compare.diff_snapshots(a, b, tolerances=[("packets.in.login", 1)])], ["packets.in.extra"])

    def test_a_pinned_clock_value_is_compared_after_all(self):
        a, b = snapshot(1, 1, 1), snapshot(1, 1, 1)
        a["level"]["day_time"], b["level"]["day_time"] = 6000, 6001
        self.assertEqual(compare.diff_snapshots(a, b), [])
        self.assertEqual([p for p, _, _ in compare.diff_snapshots(a, b, tolerances=[("level.day_time", 0)])], ["level.day_time"])

    def test_an_ignored_path_is_not_compared(self):
        a, b = snapshot(1, 1, 1), snapshot(1, 1, 1)
        a["entities"], b["entities"] = [{"type": "minecraft:item"}], []
        self.assertTrue(compare.diff_snapshots(a, b))
        self.assertEqual(compare.diff_snapshots(a, b, ignore=("entities*",)), [])

    def test_unordered_entities_compare_as_a_set(self):
        a, b = snapshot(1, 1, 1), snapshot(1, 1, 1)
        crystals = [{"id": 5, "type": "minecraft:end_crystal", "pos": [1.0, 2.0, 3.0]}, {"id": 9, "type": "minecraft:end_crystal", "pos": [7.0, 2.0, 3.0]}]
        a["entities"], b["entities"] = crystals, crystals[::-1]
        self.assertTrue(compare.diff_snapshots(a, b))
        self.assertEqual(compare.diff_snapshots(a, b, unordered=True), [])

    def test_a_value_difference_is_named_by_its_path(self):
        a, b = self.sides(snapshot(100, 5, 40), snapshot(25, 5, 40))
        self.assertEqual([k for k, _ in compare.compare_sides(a, b)], ["snapshot:joined:level.chunks_loaded"])
        self.assertEqual(compare.compare_sides(a, b, tolerances=[("level.chunks_loaded", 80)]), [])

    def test_log_and_server_differences(self):
        server = "[12:00:00] [Server thread/INFO]: Steve joined the game\n[12:00:01] [Server thread/ERROR]: boom\n"
        self.ours_log = "[12:00:00] [Render thread/INFO]: nothing\n"
        a, b = self.sides(snapshot(1, 1, 1), snapshot(1, 1, 1), server=server)
        keys = [k for k, _ in compare.compare_sides(a, b)]
        self.assertIn("client-log:missing:1x INFO: Loaded 2 advancements", keys)
        self.assertIn("client-log:extra:1x INFO: nothing", keys)
        self.assertIn("server-log:vanilla:ERROR: boom", keys)
        self.assertIn("server-log:missing:INFO: Steve joined the game", keys)

    def test_a_second_client_is_compared_on_its_own_screens_and_log(self):
        a, b = self.sides(snapshot(1, 1, 1), snapshot(1, 1, 1))
        for d, screens, log in ((a, [None], ""), (b, [None, "DisconnectedScreen"], "[12:00:00] [Render thread/WARN]: odd\n")):
            path = os.path.join(d, "side.json")
            with open(path) as f:
                meta = json.load(f)
            meta["extra"] = {"b": {"screens": screens, "disconnects": []}}
            with open(path, "w") as f:
                json.dump(meta, f)
            with open(os.path.join(d, "client-b.log"), "w") as f:
                f.write(log)
        keys = [k for k, _ in compare.compare_sides(a, b)]
        self.assertIn("screens@b", keys)
        self.assertEqual([k for k in keys if k.startswith("client-log@b:")], ["client-log@b:ours:WARN: odd", "client-log@b:extra:1x WARN: odd"])
        self.assertNotIn("screens", keys)

    def test_known_differences_hide_listed_keys_and_report_stale_ones(self):
        diffs = [("snapshot:joined:level.chunks_loaded", "x"), ("screens", "y")]
        known = {("join", "screens"), ("join", "disconnect"), ("other", "screens")}
        unexpected, listed, stale = compare.apply_known("join", diffs, known)
        self.assertEqual([k for k, _ in unexpected], ["snapshot:joined:level.chunks_loaded"])
        self.assertEqual([k for k, _ in listed], ["screens"])
        self.assertEqual(stale, ["disconnect"])

    def test_known_file_parses(self):
        path = os.path.join(self.tmp.name, "k.txt")
        with open(path, "w") as f:
            f.write("# c\n\njoin screens\njoin client-log:missing:INFO: Loaded 2 advancements\n")
        self.assertEqual(compare.load_known(path), {("join", "screens"), ("join", "client-log:missing:INFO: Loaded 2 advancements")})


if __name__ == "__main__":
    unittest.main()
