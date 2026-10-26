"""Read-only independent checks for the supplied LibreOffice regression PDF."""
from pathlib import Path
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdfium
from pypdf import PdfReader
from PIL import ImageChops

source = Path(sys.argv[1])
original_bytes = source.read_bytes()
assert hashlib.sha256(original_bytes).hexdigest() == "75c3d8487c0b7c6fb71c7f430b6e5788cd7ee485414d5473b684a178f250ab3c"
original = PdfReader(source)


def render(path, page=0):
    with pdfium.PdfDocument(path) as doc:
        bitmap = doc[page].render(scale=2)
        result = bitmap.to_pil().convert("RGB").copy()
        bitmap.close()
        return result


def text(reader):
    # Pypdf adds trailing spaces around newly translated BT/ET runs.
    return ["".join(page.extract_text().split()) for page in reader.pages]


base = render(source)
second = render(source, 1)
results = []
for filename in ["28-case-coloree.pdf", "28-tableau-deplace.pdf", "28-tableau-supprime.pdf"]:
    path = ROOT / "test-output" / filename
    reader = PdfReader(path, strict=True)
    assert len(reader.pages) == len(original.pages) == 2
    assert ImageChops.difference(second, render(path, 1)).getbbox() is None
    if filename != "28-tableau-supprime.pdf":
        assert text(reader) == text(original), filename
    else:
        assert "Sujet2" not in text(reader)[0]
        assert "Exercice1" in text(reader)[0] and "Exercice3" in text(reader)[0]
    image = render(path)
    image.resize((893, 1263)).save(path.with_suffix(".pdfium.png"))
    expected = base.copy()
    if filename == "28-tableau-deplace.pdf":
        region = (27, 153, 1152, 313)
        expected.paste((255, 255, 255), region)
        expected.paste(base.crop(region), (region[0]+10, region[1]+20))
        diff = ImageChops.difference(expected, image)
        # Font hinting at the final baseline can change a few antialiased pixels.
        pixels = sum(p != (0, 0, 0) for p in diff.get_flattened_data())
        assert pixels <= 100, f"Table movement changed {pixels} pixels beyond its translation"
        results.append({"file": filename, "difference_from_exact_pixel_translation": pixels})
    else:
        # Background recolouring / table removal may only affect their own region.
        diff = ImageChops.difference(base, image)
        allowed = (27, 153, 1152, 317) if filename.endswith("supprime.pdf") else (29, 156, 775, 192)
        diff.paste((0, 0, 0), allowed)
        assert diff.getbbox() is None, (filename, diff.getbbox())
        results.append({"file": filename, "pixels_outside_edited_region_unchanged": True})

assert source.read_bytes() == original_bytes
result = {"source_unchanged": True, "source_sha256": hashlib.sha256(original_bytes).hexdigest(), "checks": results}
(ROOT / "test-output/writer-regression-results.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result, indent=2))
