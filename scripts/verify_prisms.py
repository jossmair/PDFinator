"""Independent content, figure-position and page-count checks for the lesson."""
import argparse
import json
from pathlib import Path
from zipfile import ZipFile

from lxml import etree as E
from verify_worksheet import occurrences, tokens, pdf


def colored_bounds(page):
    im = page.render(scale=2).to_pil().convert("RGB")
    w, h = im.size
    result = {"cylinder": [w, h, 0, 0], "net": [w, h, 0, 0]}
    for i, (r, g, b) in enumerate(im.get_flattened_data()):
        x, y = i % w, i // w
        name = "cylinder" if y > h*0.75 and b > r + 35 and b > g + 15 and g < 220 else (
            "net" if h*0.35 < y < h*0.62 and r > 200 and g > 200 and b < 200 else None)
        if name:
            a = result[name]
            result[name] = [min(a[0], x), min(a[1], y), max(a[2], x+1), max(a[3], y+1)]
    return {name: [a[0]*page.get_width()/w, a[1]*page.get_height()/h,
                   a[2]*page.get_width()/w, a[3]*page.get_height()/h]
            for name, a in result.items()}


def verify(source, exports, renders):
    original = pdf.PdfDocument(str(source))
    text = original[0].get_textpage()
    expected = tokens(text.get_text_range())
    positions = {s: occurrences(text, s) for s in expected}
    figures = colored_bounds(original[0])
    for path in exports:
        with ZipFile(path) as z:
            if path.suffix == ".docx":
                root = E.fromstring(z.read("word/document.xml"))
                ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
                native = " ".join(root.xpath("//w:t/text()", namespaces=ns))
                assert not root.xpath("//w:txbxContent", namespaces=ns)
            else:
                root = E.fromstring(z.read("content.xml"))
                ns = {"text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0"}
                for space in root.xpath("//text:s | //text:tab", namespaces=ns):
                    space.text = " "
                native = " ".join("".join(p.itertext()) for p in root.xpath("//text:p", namespaces=ns))
            assert not expected - tokens(native), (path, "Native editable text", expected-tokens(native))
    report = []
    for path in renders:
        d = pdf.PdfDocument(str(path))
        assert len(d) == 1, (path, "Page count", len(d))
        actual = d[0].get_textpage()
        assert not expected-tokens(actual.get_text_range()), (path, "Missing content")
        shifts = []
        for s, rects in positions.items():
            found = occurrences(actual, s)
            for a in rects:
                assert found, (path, s)
                i = min(range(len(found)), key=lambda i: abs(found[i][0]-a[0])+abs(found[i][3]-a[3]))
                b = found.pop(i)
                shifts.append({"text": s, "dx": round(b[0]-a[0], 3), "dy": round(a[3]-b[3], 3)})
        assert max(abs(s["dx"]) for s in shifts) <= 2.5, (path, "Text x", shifts)
        assert max(abs(s["dy"]) for s in shifts) <= 1.5, (path, "Text y", shifts)
        drawn = colored_bounds(d[0])
        for name, a in figures.items():
            assert max(abs(x-y) for x, y in zip(a, drawn[name])) <= 1.5, (path, name, a, drawn[name])
        report.append({"file": str(path), "pages": 1, "tokens": sum(expected.values()),
                       "max_x": max(abs(s["dx"]) for s in shifts),
                       "max_y": max(abs(s["dy"]) for s in shifts), "figures": drawn})
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("docx", type=Path)
    parser.add_argument("odt", type=Path)
    parser.add_argument("renders", type=Path, nargs="+")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = verify(args.source, [args.docx, args.odt], args.renders)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(result))
