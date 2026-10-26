"""Regression for native content, equations, pagination and rendered positions.

The ODT is a read-only structural oracle, never an input to the converter.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys
from zipfile import ZipFile

from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdf

M = "http://www.w3.org/1998/Math/MathML"
NS = {"m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
      "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}


def normalize(s):
    return re.sub(r"\s+", "", s).replace("...", "…").replace("\ue08b", "…")


def fractions(path):
    found = Counter()
    with ZipFile(path) as z:
        if str(path).endswith(".docx"):
            root = E.fromstring(z.read("word/document.xml"))
            assert not root.xpath("//w:txbxContent", namespaces=NS)
            assert not root.xpath("//w:anchor[not(@behindDoc='1')]", namespaces=NS)
            for f in root.xpath("//m:f", namespaces=NS):
                found[tuple(normalize("".join(f.find("m:" + n, NS).itertext()))
                            for n in ["num", "den"])] += 1
            tables = root.xpath("//w:tbl[count(w:tblGrid/w:gridCol)=7 and count(w:tr)=9]", namespaces=NS)
            assert len(tables) == 1 and len(tables[0].findall("w:tr", NS)) == 9
        else:
            for name in z.namelist():
                if name.endswith("/content.xml"):
                    root = E.fromstring(z.read(name))
                    for f in root.iter("{" + M + "}mfrac"):
                        found[tuple(normalize("".join(c.itertext())) for c in f)] += 1
    return found


def tokens(text):
    return Counter(re.findall(r"\d+|[^\W\d_]+", text, re.UNICODE))


def occurrences(textpage, value):
    search = textpage.search(value, match_case=True, match_whole_word=True)
    result = []
    while (match := search.get_next()) is not None:
        a, n = match
        boxes = [textpage.get_charbox(i) for i in range(a, a + n)]
        result.append([min(b[0] for b in boxes), min(b[1] for b in boxes),
                       max(b[2] for b in boxes), max(b[3] for b in boxes)])
    return result


def render_check(source, target):
    a, b = pdf.PdfDocument(source), pdf.PdfDocument(target)
    assert len(a) == len(b) == 1, (target, "Pagination", len(b))
    ta, tb = a[0].get_textpage(), b[0].get_textpage()
    expected = tokens(ta.get_text_range())
    missing = expected - tokens(tb.get_text_range())
    assert not missing, (target, "Missing source content", missing)
    positions = []
    # Include numbers and repeated words: unique long words alone missed both
    # dropped denominators and headers broken by an unwanted line wrap.
    for value in expected:
        ra, rb = occurrences(ta, value), occurrences(tb, value)
        for rect in ra:
            if not rb:
                continue
            i = min(range(len(rb)), key=lambda i: abs(rb[i][0]-rect[0]) + abs(rb[i][3]-rect[3]))
            r = rb.pop(i)
            positions.append({"text": value, "x": rect[0], "top": 842-rect[3],
                              "dx": round(r[0]-rect[0], 3), "dy": round(rect[3]-r[3], 3),
                              "height_delta": round((r[3]-r[1])-(rect[3]-rect[1]), 3)})
    assert len(positions) > 350, len(positions)
    assert max(abs(p["dx"]) for p in positions) <= 2.5, (target, "Horizontal displacement")
    assert max(abs(p["dy"]) for p in positions) <= 1.5, (target, "Vertical displacement")
    return {"file": str(target), "pages": 1, "checked_tokens": sum(expected.values()),
            "positions": positions, "max_x": max(abs(p["dx"]) for p in positions),
            "max_y": max(abs(p["dy"]) for p in positions)}


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("source_pdf")
    p.add_argument("source_odt")
    p.add_argument("docx")
    p.add_argument("odt")
    p.add_argument("renders", nargs="+")
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    oracle = fractions(args.source_odt)
    assert sum(oracle.values()) == 14, oracle
    for path in [args.docx, args.odt]:
        actual = fractions(path)
        assert actual == oracle, (path, "Equations", oracle-actual, actual-oracle)
    results = [render_check(args.source_pdf, path) for path in args.renders]
    args.output.write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps([{k: v for k, v in r.items() if k != "positions"} for r in results]))
