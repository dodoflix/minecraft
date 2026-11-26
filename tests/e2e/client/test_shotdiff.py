"""Screenshot diff on generated frames: python3 tests/e2e/client/test_shotdiff.py"""
import contextlib
import io
import os
import sys
import random
import struct
import tempfile
import unittest
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from client_e2e import shotdiff  # noqa: E402


def run_cli(main, args):
    """Exit code of a command-line entry point, its output swallowed."""
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return main(args)


def png(width, height, pixel, kind=0, color=2):
    """A PNG written with scanline filter `kind` for every row; `pixel(x, y)` gives an RGB tuple, written with an opaque
    alpha byte for colour type 6."""
    bpp = 4 if color == 6 else 3
    rows, prev = [], bytes(width * bpp)
    for y in range(height):
        row = bytes(c for x in range(width) for c in (*pixel(x, y), *((255,) if color == 6 else ())))
        out = bytearray()
        for i, v in enumerate(row):
            a = row[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if kind == 0:
                pred = 0
            elif kind == 1:
                pred = a
            elif kind == 2:
                pred = b
            elif kind == 3:
                pred = (a + b) >> 1
            else:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
            out.append((v - pred) & 255)
        rows.append(bytes([kind]) + bytes(out))
        prev = row

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))

    return (
        shotdiff.PNG_SIGNATURE
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, color, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows)))
        + chunk(b"IEND", b"")
    )


def scene(block=(10, 10), noise=0, seed=1):
    """A frozen 64x48 frame: sky gradient, a 12x12 'block', optional per-pixel noise."""
    rng = random.Random(seed)

    def pixel(x, y):
        if block[0] <= x < block[0] + 12 and block[1] <= y < block[1] + 12:
            base = (120, 80, 40)
        else:
            base = (90 + y, 140 + x // 2, 200)
        return tuple(min(255, max(0, v + rng.randint(-noise, noise))) for v in base)

    return pixel


class ShotDiffTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def path(self, name, data):
        p = os.path.join(self.dir.name, name)
        with open(p, "wb") as f:
            f.write(data)
        return p

    def compare(self, a, b, **kw):
        return shotdiff.compare(shotdiff.read_png(a), shotdiff.read_png(b), **kw)

    def test_every_filter_type_decodes_to_the_same_pixels(self):
        pixels = [shotdiff.read_png(self.path(f"f{k}.png", png(64, 48, scene(noise=20), kind=k)))[2] for k in range(5)]
        self.assertTrue(all(p == pixels[0] for p in pixels))
        first = scene(noise=20)
        self.assertEqual(pixels[0][0], bytes(c for x in range(64) for c in first(x, 0)))

    def test_rgba_decodes_like_rgb(self):
        rgb = shotdiff.read_png(self.path("rgb.png", png(64, 48, scene(noise=9), kind=4)))
        rgba = shotdiff.read_png(self.path("rgba.png", png(64, 48, scene(noise=9), kind=4, color=6)))
        self.assertEqual(rgb, rgba)

    def test_bad_inputs_are_errors_not_tracebacks(self):
        good = png(64, 48, scene())
        ihdr_short = good[:8] + struct.pack(">I", 4) + b"IHDR" + b"\0" * 8 + good[33:]  # a 4-byte header, then the real data

        def chunk(tag, body):
            return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))

        palette_png = (
            shotdiff.PNG_SIGNATURE
            + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 1, 8, 3, 0, 0, 0))
            + chunk(b"PLTE", b"\x01\x02\x03")
            + chunk(b"IDAT", zlib.compress(b"\x00\x00\x05"))  # pixel 1 uses palette entry 5 of 1
            + chunk(b"IEND", b"")
        )
        for name, data in [("short-ihdr", ihdr_short), ("empty", png(0, 0, scene())), ("palette", palette_png)]:
            with self.assertRaises(shotdiff.PngError, msg=name):
                shotdiff.read_png(self.path(name + ".png", data))
        a = self.path("a.png", good)
        b = self.path("b.png", png(64, 48, scene(block=(14, 10))))
        for args in (["--region", "0,48,64,10"], ["--region", "0,40,64,1000"], ["--region", "60,0,10,10"], ["--grid", "99"]):
            self.assertEqual(run_cli(shotdiff.main, [a, b, *args]), 2, args)
        self.assertEqual(run_cli(shotdiff.main, [a, b, "--diff", os.path.join(self.dir.name, "no", "such", "d.png")]), 2)

    def test_identical_frames_are_equal(self):
        a = self.path("a.png", png(64, 48, scene()))
        b = self.path("b.png", png(64, 48, scene(), kind=4))
        report = self.compare(a, b)
        self.assertTrue(report["ok"])
        self.assertTrue(all(r["mean_abs_diff"] == 0 and r["changed_ratio"] == 0 for r in report["regions"]))

    def test_rendering_noise_stays_below_the_threshold(self):
        a = self.path("a.png", png(64, 48, scene(noise=2, seed=1)))
        b = self.path("b.png", png(64, 48, scene(noise=2, seed=2)))
        self.assertTrue(self.compare(a, b)["ok"])

    def test_a_moved_block_exceeds_it_and_the_diff_image_marks_it(self):
        a = self.path("a.png", png(64, 48, scene(block=(10, 10))))
        b = self.path("b.png", png(64, 48, scene(block=(14, 10))))
        rows = []
        report = self.compare(a, b, diff_rows=rows)
        self.assertFalse(report["ok"])
        self.assertTrue(any(not r["ok"] for r in report["regions"]))
        out = os.path.join(self.dir.name, "diff.png")
        shotdiff.write_png(out, 64, 48, rows)
        width, height, back = shotdiff.read_png(out)
        self.assertEqual((width, height), (64, 48))
        self.assertGreater(back[20][12 * 3], 100)  # a changed pixel is red
        self.assertEqual(back[40][60 * 3], back[40][60 * 3 + 1])  # an unchanged pixel is grey

    def test_a_crop_outside_the_change_is_equal_and_inside_it_is_not(self):
        a = self.path("a.png", png(64, 48, scene(block=(10, 10))))
        b = self.path("b.png", png(64, 48, scene(block=(14, 10))))
        self.assertTrue(self.compare(a, b, crop=(40, 0, 24, 48))["ok"])
        self.assertFalse(self.compare(a, b, crop=(0, 0, 32, 32))["ok"])

    def test_region_thresholds_can_be_loosened_per_region(self):
        a = self.path("a.png", png(64, 48, scene(block=(10, 10))))
        b = self.path("b.png", png(64, 48, scene(block=(14, 10))))
        strict = self.compare(a, b, regions=[(0, 0, 32, 32, None, None)])
        loose = self.compare(a, b, regions=[(0, 0, 32, 32, 255.0, 1.0)])
        self.assertFalse(strict["ok"])
        self.assertTrue(loose["ok"])

    def test_different_sizes_fail_with_a_message(self):
        a = self.path("a.png", png(64, 48, scene()))
        b = self.path("b.png", png(32, 48, scene()))
        report = self.compare(a, b)
        self.assertFalse(report["ok"])
        self.assertIn("sizes differ", report["error"])

    def test_cli_exit_codes_and_diff_file(self):
        a = self.path("a.png", png(64, 48, scene(block=(10, 10))))
        b = self.path("b.png", png(64, 48, scene(block=(14, 10))))
        out = os.path.join(self.dir.name, "d.png")
        self.assertEqual(run_cli(shotdiff.main, [a, a]), 0)
        self.assertEqual(run_cli(shotdiff.main, [a, b, "--diff", out]), 1)
        self.assertTrue(os.path.getsize(out) > 0)
        self.assertEqual(run_cli(shotdiff.main, [a, os.path.join(self.dir.name, "nope.png")]), 2)
        self.assertEqual(run_cli(shotdiff.main, [a, b, "--crop", "1,2,3"]), 2)

    def test_not_a_png_is_an_error(self):
        p = self.path("x.png", b"hello")
        with self.assertRaises(shotdiff.PngError):
            shotdiff.read_png(p)


if __name__ == "__main__":
    unittest.main()
