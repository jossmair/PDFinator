"""Independent PDFium/pypdf checks for native mixed selections and OCR letters."""
from pathlib import Path
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.tools/oracle'))
import pypdfium2 as pdfium
from pypdf import PdfReader
from pypdf.generic import ContentStream
from PIL import ImageChops

def render(path, page=0):
    with pdfium.PdfDocument(path) as doc:
        bitmap = doc[page].render(scale=2)
        result = bitmap.to_pil().convert('RGB').copy()
        bitmap.close()
        return result

output = ROOT / 'test-output'
base = render(output / '29-calques-original.pdf')
moved = render(output / '29-calques-deplaces.pdf')
expected = base.copy()
expected.paste((255, 255, 255), (0, 0, base.width, base.height))
expected.paste(base, (30, 24))
diff = ImageChops.difference(expected, moved)
changed = sum(p != (0, 0, 0) for p in diff.get_flattened_data())
assert changed < 350, f'Layer order/geometry changed {changed} pixels beyond the expected translation'
moved.resize((595, 842)).save(output / '29-calques-deplaces.pdfium.png')
results = {'mixed_layers_difference_pixels': changed}
multi = PdfReader(output / '33-images-multiples-modifiees.pdf', strict=True)
multi_text = multi.pages[0].extract_text()
for word in ('Nantes', 'Lyon', 'Lille'):
    assert multi_text.count(word) == 1, (word, multi_text)
assert 'Paris' not in multi_text and 'native' in multi_text
multi_image = render(output / '33-images-multiples-modifiees.pdf')
multi_original = render(output / '33-images-multiples-original.pdf')
assert ImageChops.difference(multi_image.crop((0, 0, multi_image.width, 200)),
                            multi_original.crop((0, 0, multi_original.width, 200))).getbbox() is None
multi_image.resize((595, 842)).save(output / '33-images-multiples.pdfium.png')
results['three_image_words_editable_and_searchable'] = ['Nantes', 'Lyon', 'Lille']

def paints(reader, page=0):
    return sum(op in (b'S', b's', b'f', b'F', b'f*', b'B', b'B*', b'b', b'b*', b'sh', b'BI')
               for _, op in ContentStream(reader.pages[page]['/Contents'], reader).operations)

if len(sys.argv) > 1:
    source = Path(sys.argv[1])
    original_bytes = source.read_bytes()
    source_hash = hashlib.sha256(original_bytes).hexdigest()
    assert source_hash == '75c3d8487c0b7c6fb71c7f430b6e5788cd7ee485414d5473b684a178f250ab3c'
    original = PdfReader(source)
    second = render(source, 1)
    checks = []
    for zoom in (50, 80, 140):
        path = output / f'30-selection-graphique-{zoom}.pdf'
        reader = PdfReader(path, strict=True)
        assert len(reader.pages) == len(original.pages) == 2
        assert paints(reader) == paints(original), 'Lost or duplicated native vector paint'
        for index in range(2):
            glyphs = lambda page: sorted(c for c in page.extract_text() if not c.isspace())
            assert glyphs(reader.pages[index]) == glyphs(original.pages[index]), 'Lost or duplicated glyph'
        assert ImageChops.difference(second, render(path, 1)).getbbox() is None
        a = original.pages[0]['/Resources']['/XObject']
        b = reader.pages[0]['/Resources']['/XObject']
        assert len(a) == len(b), 'Moving vectors must not create raster images or grouped XObjects'
        # Only the old and new diagram regions may change, including labels and
        # arrow tips. Unselected answer lines can cross through those regions.
        image = render(path)
        diff = ImageChops.difference(render(source), image)
        diff.paste((0, 0, 0), (625, 300, 1130, 645))
        assert diff.getbbox() is None, ('Unexpected change outside graph', diff.getbbox())
        if zoom == 80:
            image.crop((620, 280, 1140, 680)).save(output / '30-graphique.pdfium.png')
        checks.append({'zoom': zoom, 'native_paint_count': paints(reader), 'page_2_pixels_unchanged': True,
                       'glyphs_preserved_once': True, 'outside_graph_unchanged': True})
    letters = PdfReader(output / '31-lettres-schema.pdf', strict=True)
    text = letters.pages[0].extract_text()
    for letter in ('Z', 'B', 'C', 'D'):
        assert letter in text.split(), (letter, text)
    for angle in ('36°', '43°'):
        assert text.count(angle) == 1, (angle, text)
    render(output / '31-lettres-schema.pdf').crop((640, 590, 1060, 770)).save(output / '31-lettres-schema.pdfium.png')
    assert source.read_bytes() == original_bytes
    results.update(source_unchanged=True, source_sha256=source_hash, graph_checks=checks,
                   ocr_letters_editable_after_reopening=['Z', 'B', 'C', 'D'],
                   ocr_degree_signs_preserved=['36°', '43°'])

(output / 'selection-regression-results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
print(json.dumps(results, indent=2))
