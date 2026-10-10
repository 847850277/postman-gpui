"""Compare matching HTML/native control regions, without committing screenshots.

Usage: python3 tests/ui_visual_compare.py /path/to/comparison.json
Requires Pillow. Manifest: {"pairs": [{"name": "URL surface/light",
"kind": "flat", "html": {"path": "html.png", "rect": [x,y,w,h]},
"native": {"path": "native.png", "rect": [x,y,w,h]}}]}.
Capture both at 1 image pixel per logical px (1440x960 and 960x640), with
Inter/JetBrains Mono loaded and animations settled. Exclude OS decorations.
Each pair must be a local control region; never use a whole-screen threshold.
Include surface/border patches AND matching text; record focus/disabled/error
and theme in the name. Geometry/containment is separately asserted by ui_kit.rs.
"""
import json
import sys
from collections import Counter
from pathlib import Path
from PIL import Image, ImageChops, ImageFilter

# Lossless captures should be closer. Allow 8/255 for native screenshot encoding
# and color management, but reject even a single out-of-tolerance flat pixel.
FLAT_CHANNEL_TOLERANCE = 8
# Text AA is compared as ink coverage, never mixed into surface color scores.
GLYPH_EDGE_TOLERANCE = 1  # physical px at the required capture scale
GLYPH_BOUNDS_TOLERANCE = 1
MAX_UNMATCHED_INK = 0.02
MIN_INK_CONTRAST = 32


def crop(spec, root):
    image = Image.open(root / spec["path"]).convert("RGB")
    x, y, width, height = spec["rect"]
    if min(x, y) < 0 or min(width, height) <= 0 or x + width > image.width or y + height > image.height:
        raise ValueError("Comparison rectangle falls outside the screenshot")
    return image.crop((x, y, x + width, y + height))


def ink(image):
    background = Counter(image.getdata()).most_common(1)[0][0]
    mask = Image.new("L", image.size)
    mask.putdata([255 if max(abs(a-b) for a, b in zip(pixel, background)) >= MIN_INK_CONTRAST else 0 for pixel in image.getdata()])
    return mask


def compare(left, right, kind):
    if left.size != right.size:
        raise AssertionError(f"Region sizes differ: {left.size} != {right.size}")
    if kind == "flat":
        error = max(channel[1] for channel in ImageChops.difference(left, right).getextrema())
        if error > FLAT_CHANNEL_TOLERANCE:
            raise AssertionError(f"Local color difference {error}/255 > {FLAT_CHANNEL_TOLERANCE}/255")
        return {"maximum_channel_difference": error}
    if kind != "glyph":
        raise ValueError("kind must be flat or glyph")
    a, b = ink(left), ink(right)
    if not a.getbbox() or not b.getbbox():
        raise AssertionError("Text region must contain visible ink in both captures")
    bounds_error = max(abs(x-y) for x, y in zip(a.getbbox(), b.getbbox()))
    if bounds_error > GLYPH_BOUNDS_TOLERANCE:
        raise AssertionError(f"Glyph bounds differ by {bounds_error}px")
    unmatched = []
    for source, target in [(a, b), (b, a)]:
        expanded = target.filter(ImageFilter.MaxFilter(2 * GLYPH_EDGE_TOLERANCE + 1))
        total = sum(source.getdata())
        if total < 10 * 255:
            raise AssertionError("Text region is too small to compare")
        unmatched.append(sum(ImageChops.subtract(source, expanded).getdata()) / total)
    if max(unmatched) > MAX_UNMATCHED_INK:
        raise AssertionError(f"Unmatched glyph ink: {max(unmatched):.2%}")
    return {"glyph_bounds_difference_px": bounds_error, "unmatched_ink": max(unmatched)}


def main(path):
    manifest = json.loads(path.read_text())
    kinds = {pair["kind"] for pair in manifest["pairs"]}
    if kinds != {"flat", "glyph"}:
        raise ValueError("Evidence must cover both solid colors and text")
    results = []
    for pair in manifest["pairs"]:
        result = {"name": pair["name"]}
        try:
            result.update(compare(crop(pair["html"], path.parent), crop(pair["native"], path.parent), pair["kind"]))
            result["passed"] = True
        except (ValueError, AssertionError) as error:
            result.update(passed=False, error=str(error))
        results.append(result)
    print(json.dumps(results, indent=2))
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1])))
