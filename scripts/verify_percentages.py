"""Check editable fractions, native text and rendered positions against the supplied PDF."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import sys
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdf
import pypdfium2.raw as raw
from lxml import etree as E
from verify_worksheet import occurrences, tokens

source = Path(sys.argv[1])
original = source.read_bytes()
doc = pdf.PdfDocument(source)
text = doc[0].get_textpage()
expected = Counter({("12", "100"): 1, ("70", "100"): 2,
                    ("70×450", "100"): 1, ("126", "450"): 1})
ns = {"m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
      "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
math = "{http://www.w3.org/1998/Math/MathML}"
def normalize(s):
    return re.sub(r"\s+", "", s)

for extension in ("docx", "odt"):
    path = ROOT / f"test-output/47-percentages.{extension}"
    equations = Counter()
    with ZipFile(path) as z:
        assert z.testzip() is None
        if extension == "docx":
            tree = E.fromstring(z.read("word/document.xml"))
            assert not tree.xpath("//w:txbxContent", namespaces=ns)
            for f in tree.xpath("//m:f", namespaces=ns):
                equations[tuple(normalize("".join(f.find("m:"+part, ns).itertext()))
                                for part in ("num", "den"))] += 1
            for f in tree.xpath('//w:sdt[w:sdtPr/w:tag[@w:val="PDFinatorFraction"]]', namespaces=ns):
                parts = f.xpath('w:sdtContent/w:p', namespaces=ns)
                assert len(parts) == 2
                equations[tuple(normalize(''.join(p.itertext())) for p in parts)] += 1
        else:
            for name in z.namelist():
                if name.endswith("/content.xml"):
                    for f in E.fromstring(z.read(name)).iter(math+"mfrac"):
                        equations[tuple(normalize("".join(c.itertext())) for c in f)] += 1
        assert equations == expected, (path.name, equations)

reports = []
figure_frames = [(413, 257, 530, 312), (438, 565, 532, 616)]
def in_figure(rect):
    x = (rect[0] + rect[2]) / 2
    y = doc[0].get_height() - (rect[1] + rect[3]) / 2
    return any(a <= x <= c and b <= y <= d for a, b, c, d in figure_frames)

anchors = ["Pourcentage", "Exemples", "II.", "Exemple", "Combien",
           "On cherche", "Le coefficient", "Rappel", "III.", "Quel", "Il y a donc"]
for name, path in [("docx", ROOT / "test-output/47-render-docx/47-percentages.pdf"),
                   ("odt", ROOT / "test-output/47-after-odt/47-percentages.pdf")]:
    output = pdf.PdfDocument(path)
    assert len(output) == len(doc) == 1
    actual = output[0].get_textpage()
    checked = []
    for value in anchors:
        before = occurrences(text, value)
        after = occurrences(actual, value)
        assert len(before) == len(after), (name, value, len(before), len(after))
        for a in before:
            i = min(range(len(after)), key=lambda i: abs(after[i][0]-a[0])+abs(after[i][3]-a[3]))
            b = after.pop(i)
            checked.append({"text": value, "dx": b[0]-a[0], "dy": a[3]-b[3]})
    max_x = max(abs(p["dx"]) for p in checked)
    max_y = max(abs(p["dy"]) for p in checked)
    assert max_x <= 1 and max_y <= 1, (name, checked)
    # Repeated numeric glyphs also matter: prose-only checks missed shrunken fractions.
    source_tokens = tokens(''.join(text.get_text_range(i, 1)
                                  for i in range(text.count_chars())
                                  if not in_figure(text.get_charbox(i))))
    assert not source_tokens - tokens(actual.get_text_range()), name
    numeric = []
    for value in source_tokens:
        if not value.isdigit():
            continue
        after = occurrences(actual, value)
        for a in occurrences(text, value):
            if in_figure(a):
                continue
            assert after, (name, value)
            i = min(range(len(after)), key=lambda i: abs(after[i][0]-a[0])+abs(after[i][3]-a[3]))
            b = after.pop(i)
            numeric.append((value, b[0]-a[0], a[3]-b[3]))
    assert max(abs(v[1]) for v in numeric) <= 2.5, (name, numeric)
    assert max(abs(v[2]) for v in numeric) <= 2, (name, numeric)
    sizes = []
    search = actual.search('70', match_whole_word=True)
    while (match := search.get_next()) is not None:
        y = doc[0].get_height() - actual.get_charbox(match[0])[3]
        if 453 < y < 460:
            sizes.append(raw.FPDFText_GetFontSize(actual, match[0]))
    assert len(sizes) == 2 and all(abs(size - 15.5) <= 1 for size in sizes), (name, sizes)
    # The curves and their multiplier labels remain next to their tables.
    a = doc[0].render(scale=2).to_pil().convert("RGB")
    b = output[0].render(scale=2).to_pil().convert("RGB")
    for frame in figure_frames:
        bounds = []
        box = tuple(int(v*2) for v in frame)
        for image in (a, b):
            crop = image.crop(box).convert("L").point(lambda v: 255 if v < 150 else 0)
            bounds.append(crop.getbbox())
        assert all(bounds), (name, frame)
        assert max(abs(x-y)/2 for x, y in zip(*bounds)) <= 2, (name, frame, bounds)
    # Nested columns must leave room for the tables' right-hand borders.
    for x, top, bottom in [(409.5, 266.8, 305.5), (435, 572.1, 610.8)]:
        samples = range(int((top+1)*2), int((bottom-1)*2))
        strength = max(sum(max(b.getpixel((px, py))) < 210 for py in samples) / len(samples)
                       for px in range(int((x-2.5)*2), int((x+2.5)*2)+1))
        assert strength >= 0.85, (name, 'clipped table border', x, strength)
    reports.append({"format": name, "pages": 1, "editable_fractions": 5,
                    "checked_anchors": len(checked), "max_x_pt": max_x, "max_y_pt": max_y,
                    "checked_numeric_tokens": len(numeric), "large_fraction_sizes_pt": sizes,
                    "max_numeric_x_pt": max(abs(v[1]) for v in numeric),
                    "max_numeric_y_pt": max(abs(v[2]) for v in numeric),
                    "figures_next_to_tables": True})
assert source.read_bytes() == original
result = {"source_sha256": hashlib.sha256(original).hexdigest(), "results": reports}
(ROOT / "test-output/47-verification.json").write_text(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))
