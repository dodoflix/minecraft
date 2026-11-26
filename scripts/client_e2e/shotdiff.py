#!/usr/bin/env python3
"""Compare two screenshots region by region (stdlib only).

    shotdiff.py A.png B.png [--crop X,Y,W,H] [--grid N | --region X,Y,W,H[,MEAN[,RATIO]]...]
                [--mean-threshold F] [--ratio-threshold F] [--pixel-tolerance N] [--diff OUT.png]

Both images are cropped (--crop, default: whole image), then split into an N x N grid (--grid, default 4) or into the
explicit --region rectangles (coordinates relative to the crop; optional per-region thresholds). For every region it
reports the mean absolute channel difference (0..255) and the ratio of changed pixels (a pixel is changed when any
channel differs by more than --pixel-tolerance, default 8). The comparison fails when any region exceeds its mean
threshold (default 2.0) or its ratio threshold (default 0.02). --diff writes a PNG: changed pixels red, scaled by the
difference, unchanged ones dimmed.

Prints one JSON object (ok, size, regions[]); exit 0 iff ok, 1 on a difference or on images of different size, 2 on
usage errors and unreadable files. Screenshots are only comparable when taken on the same machine in the same run
(GPU and driver change rendering), which is how the end-to-end runner uses it.
"""
import json
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}  # PNG colour type -> samples per pixel


class PngError(Exception):
    pass


def _unfilter(raw, width, height, bpp):
    """Reverses the PNG scanline filters; returns the list of unfiltered rows (bytes)."""
    stride = width * bpp
    rows = []
    prev = bytearray(stride)
    pos = 0
    for _ in range(height):
        if pos + 1 + stride > len(raw):
            raise PngError("truncated image data")
        kind = raw[pos]
        line = bytearray(raw[pos + 1 : pos + 1 + stride])
        pos += 1 + stride
        if kind == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif kind == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif kind == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif kind == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 255
        elif kind != 0:
            raise PngError(f"unknown filter type {kind}")
        rows.append(bytes(line))
        prev = line
    return rows


def read_png(path):
    """Returns (width, height, rows) with every row as width*3 RGB bytes (alpha dropped, grey and palette expanded)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != PNG_SIGNATURE:
        raise PngError(f"{path}: not a PNG file")
    pos, header, palette, idat = 8, None, b"", []
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body) if len(body) == 13 else ()
        elif kind == b"PLTE":
            palette = body
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None or not idat:
        raise PngError(f"{path}: missing IHDR or IDAT")
    if not header:
        raise PngError(f"{path}: bad IHDR")
    width, height, depth, color, _, _, interlace = header
    if width == 0 or height == 0:
        raise PngError(f"{path}: empty image")
    if depth != 8 or color not in CHANNELS or interlace:
        raise PngError(f"{path}: only 8-bit non-interlaced PNGs are supported (depth {depth}, type {color}, interlace {interlace})")
    bpp = CHANNELS[color]
    try:
        raw = zlib.decompress(b"".join(idat))
    except zlib.error as e:
        raise PngError(f"{path}: bad image data: {e}") from e
    rows = _unfilter(raw, width, height, bpp)
    if color == 2:
        return width, height, rows
    out = []
    for row in rows:
        rgb = bytearray(width * 3)
        for x in range(width):
            if color == 6:
                rgb[x * 3 : x * 3 + 3] = row[x * 4 : x * 4 + 3]
            elif color == 3:
                i = row[x] * 3
                if i + 3 > len(palette):
                    raise PngError(f"{path}: palette index {row[x]} is outside PLTE")
                rgb[x * 3 : x * 3 + 3] = palette[i : i + 3]
            else:  # grey (+ alpha)
                g = row[x * bpp]
                rgb[x * 3 : x * 3 + 3] = bytes((g, g, g))
        out.append(bytes(rgb))
    return width, height, out


def write_png(path, width, height, rows):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    raw = b"".join(b"\x00" + row for row in rows)
    with open(path, "wb") as f:
        f.write(PNG_SIGNATURE)
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 6)))
        f.write(chunk(b"IEND", b""))


def _parse_rect(text, what):
    try:
        x, y, w, h = (int(v) for v in text.split(","))
    except ValueError:
        raise ValueError(f"{what} wants X,Y,W,H integers, got {text!r}") from None
    if x < 0 or y < 0 or w <= 0 or h <= 0:
        raise ValueError(f"{what} {text!r}: needs x,y >= 0 and w,h > 0")
    return x, y, w, h


def _crop(image, rect):
    width, height, rows = image
    if rect is None:
        return image
    x, y, w, h = rect
    if x + w > width or y + h > height:
        raise ValueError(f"crop {x},{y},{w},{h} is outside the {width}x{height} image")
    return w, h, [row[x * 3 : (x + w) * 3] for row in rows[y : y + h]]


def crop_file(path, rect):
    """Replaces the PNG at `path` by its crop (x, y, w, h)."""
    width, height, rows = _crop(read_png(path), rect)
    write_png(path, width, height, rows)


def compare(a, b, crop=None, grid=4, regions=None, mean_threshold=2.0, ratio_threshold=0.02, pixel_tolerance=8, diff_rows=None):
    """Compares two decoded images. `regions` (list of (x, y, w, h, mean, ratio), thresholds may be None) replaces the grid.

    `diff_rows`, when a list, receives the rows of the diff image."""
    if a[:2] != b[:2]:
        return {"ok": False, "size": [a[0], a[1]], "error": f"image sizes differ: {a[0]}x{a[1]} vs {b[0]}x{b[1]}", "regions": []}
    width, height, rows_a = _crop(a, crop)
    rows_b = _crop(b, crop)[2]
    if regions is None:
        if grid < 1 or grid > min(width, height):
            raise ValueError(f"--grid needs a number from 1 to {min(width, height)} for a {width}x{height} image")
        xs = [width * i // grid for i in range(grid + 1)]
        ys = [height * i // grid for i in range(grid + 1)]
        regions = [(xs[i], ys[j], xs[i + 1] - xs[i], ys[j + 1] - ys[j], None, None) for j in range(grid) for i in range(grid)]
    for rx, ry, rw, rh, _, _ in regions:
        if rx + rw > width or ry + rh > height:
            raise ValueError(f"region {rx},{ry},{rw},{rh} is outside the {width}x{height} compared area")
    stats = [[0, 0] for _ in regions]  # [sum of channel differences, changed pixels]
    out = []
    for y in range(height):
        ra, rb = rows_a[y], rows_b[y]
        diff = [abs(p - q) for p, q in zip(ra, rb)]
        if diff_rows is not None:
            line = bytearray(width * 3)
            for x in range(width):
                d = max(diff[x * 3 : x * 3 + 3])
                if d > pixel_tolerance:
                    line[x * 3] = min(255, 64 + d * 4)
                else:
                    g = (ra[x * 3] + ra[x * 3 + 1] + ra[x * 3 + 2]) // 9
                    line[x * 3 : x * 3 + 3] = bytes((g, g, g))
            out.append(bytes(line))
        for n, (rx, ry, rw, rh, _, _) in enumerate(regions):
            if ry <= y < ry + rh:
                seg = diff[rx * 3 : (rx + rw) * 3]
                stats[n][0] += sum(seg)
                stats[n][1] += sum(1 for x in range(rw) if max(seg[x * 3 : x * 3 + 3]) > pixel_tolerance)
    if diff_rows is not None:
        diff_rows.extend(out)
    report, ok = [], True
    for (rx, ry, rw, rh, mean_limit, ratio_limit), (total, changed) in zip(regions, stats):
        mean_limit = mean_threshold if mean_limit is None else mean_limit
        ratio_limit = ratio_threshold if ratio_limit is None else ratio_limit
        mean, ratio = total / (rw * rh * 3), changed / (rw * rh)
        passed = mean <= mean_limit and ratio <= ratio_limit
        ok &= passed
        report.append(
            {"rect": [rx, ry, rw, rh], "mean_abs_diff": round(mean, 4), "changed_ratio": round(ratio, 5),
             "mean_limit": mean_limit, "ratio_limit": ratio_limit, "ok": passed}
        )
    return {"ok": ok, "size": [width, height], "regions": report}


def _parse_args(argv):
    opts = {"crop": None, "grid": 4, "regions": [], "mean": 2.0, "ratio": 0.02, "tolerance": 8, "diff": None}
    files, i = [], 0
    while i < len(argv):
        arg = argv[i]
        if arg in ("-h", "--help"):
            sys.stdout.write(__doc__)
            sys.exit(0)
        if not arg.startswith("--"):
            files.append(arg)
            i += 1
            continue
        if i + 1 >= len(argv):
            raise ValueError(f"{arg} needs a value")
        value = argv[i + 1]
        i += 2
        if arg == "--crop":
            opts["crop"] = _parse_rect(value, "--crop")
        elif arg == "--grid":
            opts["grid"] = int(value)
        elif arg == "--region":
            parts = value.split(",")
            if len(parts) not in (4, 5, 6):
                raise ValueError(f"--region wants X,Y,W,H[,MEAN[,RATIO]], got {value!r}")
            rect = _parse_rect(",".join(parts[:4]), "--region")
            limits = [float(p) for p in parts[4:]] + [None, None]
            opts["regions"].append(rect + (limits[0], limits[1]))
        elif arg == "--mean-threshold":
            opts["mean"] = float(value)
        elif arg == "--ratio-threshold":
            opts["ratio"] = float(value)
        elif arg == "--pixel-tolerance":
            opts["tolerance"] = int(value)
        elif arg == "--diff":
            opts["diff"] = value
        else:
            raise ValueError(f"unknown option {arg}")
    if len(files) != 2:
        raise ValueError("needs exactly two PNG files")
    return files, opts


def main(argv):
    try:
        (path_a, path_b), o = _parse_args(argv)
        a, b = read_png(path_a), read_png(path_b)
        rows = [] if o["diff"] else None
        result = compare(a, b, o["crop"], o["grid"], o["regions"] or None, o["mean"], o["ratio"], o["tolerance"], rows)
        if rows:
            write_png(o["diff"], result["size"][0], result["size"][1], rows)
    except (ValueError, PngError, OSError) as e:
        sys.stderr.write(f"shotdiff: {e}\n")
        return 2
    result["a"], result["b"] = path_a, path_b
    sys.stdout.write(json.dumps(result) + "\n")
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
