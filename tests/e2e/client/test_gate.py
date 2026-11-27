"""Client log gate on recorded logs: python3 tests/e2e/client/test_gate.py"""
import contextlib
import io
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from client_e2e import gate  # noqa: E402


def run_cli(main, args):
    """Exit code of a command-line entry point, its output swallowed."""
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return main(args)


def read(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


class GateTest(unittest.TestCase):
    """fixtures/*.txt: client logs of one scenario (join, stay 20 s) against the vanilla server and against an old build of ours."""

    vanilla = read("fixtures/vanilla-join.txt")
    ours = read("fixtures/ours-join.txt")

    def test_both_layouts_parse(self):
        console = gate.parse("[12:00:00] [Render thread/INFO]: hello\n[12:00:01] [Render thread/WARN]: careful")
        file_log = gate.parse("[12:00:00] [Render thread/INFO] [n.m.c.Minecraft]: hello")
        self.assertEqual([e.line for e in console], ["INFO: hello", "WARN: careful"])
        self.assertEqual(file_log[0].message, "hello")
        self.assertEqual(gate.parse("[gamescope] [Error]  vulkan: nothing\nplain text"), [])

    def test_the_noise_list_removes_exactly_its_lines_from_the_recorded_findings(self):
        before, _ = gate.findings(gate.parse(self.vanilla))
        self.assertEqual(len(before), 5, before)
        noise = gate.load_allow(os.path.join(HERE, "client-log-noise.txt"))
        found, unused = gate.findings(gate.parse(self.vanilla), noise)
        # what the old, unisolated harness logged and the noise list rightly does not hide
        self.assertEqual(
            found,
            [
                "ERROR: Failed to fetch user properties | com.mojang.authlib.exceptions.InvalidCredentialsException: Status: 401",
                "INFO: Could not authorize you against Realms server: java.lang.RuntimeException: Failed to parse into SignedJWT: 0",
                "ERROR: Failed to fetch Realms feature flags | com.mojang.realmsclient.exception.RealmsServiceException: "
                "Realms authentication error with message 'java.lang.RuntimeException: Failed to parse into SignedJWT: 0'",
            ],
        )
        # every noise line either removed a finding of this older recording or is unused (the lines of the isolated harness and
        # of some scenarios are not in it)
        removed = [f for f in before if f not in found]
        self.assertEqual(sorted(removed + unused), sorted(noise))

    def test_allow_list_is_exact(self):
        entries = gate.parse("[12:00:00] [Render thread/WARN]: Requested post effect does not exist: minecraft:end_of_frame2")
        found, unused = gate.findings(entries, ["WARN: Requested post effect does not exist: minecraft:end_of_frame"])
        self.assertEqual(found, ["WARN: Requested post effect does not exist: minecraft:end_of_frame2"])
        self.assertEqual(len(unused), 1)

    def test_a_decode_failure_at_debug_level_is_a_finding(self):
        text = (
            "[12:00:00] [Render thread/DEBUG] [n.m.n.Connection]: fine\n"
            "[12:00:01] [Render thread/DEBUG] [n.m.n.Connection]: Skipping packet due to errors\n"
            "io.netty.handler.codec.DecoderException: Failed to decode packet\n"
            "\tat io.netty.Foo.decode(Foo.java:1)\n"
        )
        found, _ = gate.findings(gate.parse(text))
        self.assertEqual(found, ["DEBUG: Skipping packet due to errors | io.netty.handler.codec.DecoderException: Failed to decode packet"])

    def test_isolation_breach_and_info_exception_are_findings(self):
        text = (
            '[12:00:00] [e2e-watch/INFO]: [STDOUT]: [E2E-NET] {"blocked": "sessionserver.mojang.com"}\n'
            "[12:00:01] [Render thread/INFO]: java.lang.RuntimeException: boom\n"
            "[12:00:02] [Render thread/INFO]: all good\n"
        )
        self.assertEqual(len(gate.findings(gate.parse(text))[0]), 2)

    def test_the_missing_advancements_line_of_the_old_build_is_reported(self):
        report = gate.gate(self.ours, self.vanilla)
        self.assertEqual(report["parity"]["missing"], ["1x INFO: Loaded 2 advancements"])
        self.assertEqual(report["parity"]["extra"], [])
        self.assertFalse(report["ok"])

    def test_a_log_equals_itself_and_run_noise_is_not_a_difference(self):
        self.assertEqual(gate.parity(gate.parse(self.vanilla), gate.parse(self.vanilla)), {"missing": [], "extra": []})
        a = "[12:00:00] [Render thread/INFO]: Connecting to 127.0.0.1, 25571\n[12:00:01] [Render thread/INFO]: Resizing X buffer"
        b = "[13:00:00] [Render thread/INFO]: Connecting to 127.0.0.1, 25580"
        self.assertEqual(gate.parity(gate.parse(a), gate.parse(b)), {"missing": [], "extra": []})

    def test_report_files_are_findings(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(gate.extra_files([d]), [])
            open(os.path.join(d, "crash-2026.txt"), "w").close()
            self.assertEqual(len(gate.extra_files([d])), 1)
            self.assertFalse(gate.gate("", extra_dirs=[d])["ok"])

    def test_cli_exit_codes(self):
        with tempfile.TemporaryDirectory() as d:
            ok, bad = os.path.join(d, "ok.log"), os.path.join(d, "bad.log")
            with open(ok, "w") as f:
                f.write("[12:00:00] [Render thread/INFO]: fine\n")
            with open(bad, "w") as f:
                f.write("[12:00:00] [Render thread/ERROR]: broken\n")
            self.assertEqual(run_cli(gate.main, [ok]), 0)
            self.assertEqual(run_cli(gate.main, [bad]), 1)
            self.assertEqual(run_cli(gate.main, [os.path.join(d, "missing.log")]), 2)
            self.assertEqual(run_cli(gate.main, [bad, ok]), 2)  # a second log without --baseline is a mistake


if __name__ == "__main__":
    unittest.main()
