"""Verify moved native diagonal polygons without changing the source PDF."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdfium
from pypdf import PdfReader
from PIL import ImageChops, ImageDraw

source_path = Path(sys.argv[1])
source_hash = hashlib.sha256(source_path.read_bytes()).hexdigest()
source = pdfium.PdfDocument(source_path)
scale = 2
original = source[0].render(scale=scale).to_pil().convert("RGB")
second = source[1].render(scale=1).to_pil().convert("RGB")
native = source[0].get_textpage().get_text_range()
cases = []
for metadata in sorted((ROOT / "test-output").glob("46-figuresexo-*.json")):
    data = json.loads(metadata.read_text())
    path = metadata.with_suffix(".pdf")
    reader = PdfReader(path, strict=True)
    doc = pdfium.PdfDocument(path)
    assert len(reader.pages) == len(doc) == len(source)
    assert doc[0].get_textpage().get_text_range() == native
    assert ImageChops.difference(second, doc[1].render(scale=1).to_pil().convert("RGB")).getbbox() is None
    image = doc[0].render(scale=scale).to_pil().convert("RGB")
    diff = ImageChops.difference(original, image)
    mask = ImageDraw.Draw(diff)
    for name in ("old", "new"):
        drawing = data[name]
        a = drawing["a"]
        b = [a[i] + drawing["u"][i] for i in (0, 1)]
        pad = drawing["width"] + 3
        mask.rectangle((int((min(a[0], b[0]) - pad) * scale),
                        int((data["page_height"] - max(a[1], b[1]) - pad) * scale),
                        int((max(a[0], b[0]) + pad) * scale) + 1,
                        int((data["page_height"] - min(a[1], b[1]) + pad) * scale) + 1), fill=0)
    assert diff.getbbox() is None, (path.name, "Unselected content changed", diff.getbbox())
    red = sum(r > 170 and g < 80 and b < 110 for r, g, b in image.getdata())
    assert red > 100, (path.name, red)
    cases.append({"file": path.name, "red_stroke_pixels": red,
                  "unselected_pixels_identical": True, "other_page_identical": True,
                  "native_text_unchanged": True})
assert len(cases) == 4
assert hashlib.sha256(source_path.read_bytes()).hexdigest() == source_hash
result = {"source_sha256": source_hash, "cases": cases}
(ROOT / "test-output/46-independent-qa.json").write_text(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))
