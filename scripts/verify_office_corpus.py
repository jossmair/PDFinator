"""Independent checks on rendered synthetic Office exports (development only)."""
import argparse
import json
from pathlib import Path
from zipfile import ZipFile
import xml.etree.ElementTree as E
import pypdfium2 as pdf

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory
ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
      "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
      "style": "urn:oasis:names:tc:opendocument:xmlns:style:1.0"}

def characters(path):
    with pdf.PdfDocument(path) as d:
        assert len(d) == 1
        t = d[0].get_textpage()
        return [(t.get_text_range(i, 1), t.get_charbox(i))
                for i in range(t.count_chars()) if not t.get_text_range(i, 1).isspace()]

source = characters(root / "scripts.pdf")
report = []
for fmt in ("docx", "odt"):
    with ZipFile(root / f"scripts-after.{fmt}") as z:
        assert z.testzip() is None
        if fmt == "docx":
            tree = E.fromstring(z.read("word/document.xml"))
            runs = [r for r in tree.findall(".//w:r", ns)
                    if "".join(r.itertext()).strip() == "2"]
            assert len(runs) == 2
            assert [r.find("w:rPr/w:position", ns).get(f"{{{ns['w']}}}val") for r in runs] == ["-8", "12"]
            assert all(r.find("w:rPr/w:vertAlign", ns) is None for r in runs)
        else:
            tree = E.fromstring(z.read("content.xml"))
            styles = {s.get(f"{{{ns['style']}}}name"): s for s in tree.findall(".//style:style", ns)}
            runs = [r for r in tree.findall(".//text:span", ns) if "".join(r.itertext()).strip() == "2"]
            assert len(runs) == 2
            for r in runs:
                properties = styles[r.get(f"{{{ns['text']}}}style-name")].find("style:text-properties", ns)
                assert properties.get(f"{{{ns['style']}}}text-position").endswith(" 100%")
    actual = characters(root.parent / f"office-after-{fmt}/scripts-after.pdf")
    assert [c for c, _ in source] == [c for c, _ in actual], "Editable text must be preserved"
    for (ch, a), (_, b) in zip(source, actual):
        # This regression guards size, not just the presence of an exponent.
        if ch == "2":
            assert abs((a[3] - a[1]) - (b[3] - b[1])) < 0.15
            assert abs((a[2] - a[0]) - (b[2] - b[0])) < 0.15
    source_two = [b for c, b in source if c == "2"]
    actual_two = [b for c, b in actual if c == "2"]
    source_base = [b for c, b in source if c in ("H", "x")][-2:]
    actual_base = [b for c, b in actual if c in ("H", "x")][-2:]
    errors = [abs((a[1] - b[1]) - (c[1] - d[1]))
              for a, b, c, d in zip(source_two, source_base, actual_two, actual_base)]
    assert max(errors) < 1., "Script baseline offset changed unexpectedly"
    with pdf.PdfDocument(root / "mixed-pages.pdf") as original, \
            pdf.PdfDocument(root.parent / f"office-after-{fmt}/mixed-pages.pdf") as converted:
        assert len(original) == len(converted) == 3
        for a, b in zip(original, converted):
            assert max(abs(x - y) for x, y in zip(a.get_size(), b.get_size())) < 0.5
        assert not converted[1].get_textpage().get_text_range().strip(), "Keep the middle blank landscape page"
    report.append({"format": fmt, "script_size_preserved": True,
                   "max_relative_baseline_error_pt": max(errors), "mixed_pages": 3})
(root / "verification.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report))
