"""Compare actual exported ink positions with the original PDF (QA only)."""
import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdf
from PIL import Image, ImageChops


def bounds(textpage, word):
    search = textpage.search(word, match_case=True, match_whole_word=True)
    found = search.get_next()
    if found is None or search.get_next() is not None:
        return None
    start, count = found
    boxes = [textpage.get_charbox(i) for i in range(start, start + count)]
    return [min(b[0] for b in boxes), min(b[1] for b in boxes),
            max(b[2] for b in boxes), max(b[3] for b in boxes)]


def compare(source, target, directory, max_position_error=None):
    original, exported = pdf.PdfDocument(source), pdf.PdfDocument(target)
    assert len(original) == len(exported), (target, len(original), len(exported))
    directory.mkdir(parents=True, exist_ok=True)
    report = []
    for i in range(len(original)):
        a, b = original[i], exported[i]
        assert all(abs(x - y) < 0.5 for x, y in zip(a.get_size(), b.get_size()))
        ta, tb = a.get_textpage(), b.get_textpage()
        words = sorted(set(re.findall(r"[^\W\d_]{4,}", ta.get_text_range())))
        positions = []
        for word in words:
            ra, rb = bounds(ta, word), bounds(tb, word)
            if ra and rb:
                positions.append({"text": word, "dx": round(rb[0] - ra[0], 3),
                                  "dy": round(ra[3] - rb[3], 3),
                                  "width_delta": round((rb[2]-rb[0])-(ra[2]-ra[0]), 3)})
        im_a = a.render(scale=2).to_pil().convert("RGB")
        im_b = b.render(scale=2).to_pil().convert("RGB")
        if im_b.size != im_a.size:
            im_b = im_b.resize(im_a.size)
        Image.blend(im_a, im_b, 0.5).save(directory / f"overlay-{i+1}.png")
        ImageChops.difference(im_a, im_b).save(directory / f"difference-{i+1}.png")
        shifts = sorted(abs(p["dy"]) for p in positions)
        if max_position_error is not None:
            assert len(positions) >= 20, (i + 1, "Insufficient matched source words")
            assert max(shifts) <= max_position_error, (i+1, max(shifts), max_position_error)
            assert max(abs(p["dx"]) for p in positions) <= max_position_error
            assert max(abs(p["width_delta"]) for p in positions) <= max_position_error
        report.append({"page": i + 1, "matched_words": len(positions),
                       "median_y": shifts[len(shifts)//2] if shifts else None,
                       "max_y": max(shifts, default=None), "positions": positions})
    (directory / "geometry.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps([{k: v for k, v in p.items() if k != "positions"} for p in report]))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("source")
    p.add_argument("target")
    p.add_argument("output", type=Path)
    p.add_argument("--max-position-error", type=float)
    args = p.parse_args()
    compare(args.source, args.target, args.output, args.max_position_error)
