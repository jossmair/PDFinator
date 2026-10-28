"""Independently verify editable fractions and actual LibreOffice PDF rendering."""
import argparse
import json
from pathlib import Path
from zipfile import ZipFile
from lxml import etree as E
import pypdfium2 as pdf

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory
ns = {"m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
      "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
math = {"m": "http://www.w3.org/1998/Math/MathML"}
results = []
for name, numerator in [("positive", "12"), ("unicode-minus", "−12")]:
    for fmt in ["docx", "odt"]:
        with ZipFile(root / f"{name}.{fmt}") as z:
            assert z.testzip() is None
            if fmt == "docx":
                tree = E.fromstring(z.read("word/document.xml"))
                fractions = tree.xpath("//m:f", namespaces=ns)
                assert len(fractions) == 1, (name, "editable Word fraction")
                assert "".join(fractions[0].xpath("m:num//m:t/text()", namespaces=ns)) == numerator
                assert "".join(fractions[0].xpath("m:den//m:t/text()", namespaces=ns)) == "7"
                assert not tree.xpath("//w:txbxContent", namespaces=ns)
            else:
                paths = [n for n in z.namelist() if n.startswith("Formula") and n.endswith("/content.xml")]
                assert len(paths) == 1
                tree = E.fromstring(z.read(paths[0]))
                fractions = tree.xpath("//m:mfrac", namespaces=math)
                assert len(fractions) == 1
                assert "".join(fractions[0][0].itertext()) == numerator
                assert "".join(fractions[0][1].itertext()) == "7"
        with pdf.PdfDocument(root / f"rendered-{fmt}" / f"{name}.pdf") as d:
            assert len(d) == 1, (name, fmt, "pagination")
            page = d[0]
            text = page.get_textpage().get_text_range()
            assert numerator in "".join(text.split()) and "7" in text, (name, fmt, "missing text")
            image = page.render(scale=3).to_pil().convert("L")
            ink = image.point(lambda v: 255 if v < 90 else 0).getbbox()
            assert ink is not None, (name, fmt, "blank output")
            # Between numerator and denominator: the minus glyph cannot pass
            # this check on its own. The original missing bars fail here.
            longest = 0
            for y in range(125, 151):
                run = 0
                for x in range(210, 365):
                    run = run + 1 if image.getpixel((x, y)) < 90 else 0
                    longest = max(longest, run)
            assert longest >= 30, (name, fmt, "missing fraction bar", longest)
            image.save(root / f"rendered-{fmt}" / f"{name}.png")
            results.append({"case": name, "format": fmt, "pages": 1,
                            "editable_fraction": True, "ink_bbox_px": ink,
                            "bar_run_px": longest})
(root / "verification.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
print(json.dumps(results))
