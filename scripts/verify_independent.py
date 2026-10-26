"""Development-only validation with PDFium and pypdf; neither ships in PDFinator."""
from pathlib import Path
import json
import struct
import sys
from ctypes import byref, c_uint, c_int, create_string_buffer

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools/oracle"))
import pypdfium2 as pdfium
import pypdfium2.raw as pdfium_raw
from pypdf import PdfReader
from pypdf.generic import ContentStream

results = []
for path in sorted((ROOT / "test-output").glob("*.pdf")):
    reader = PdfReader(path, strict=True)
    document = pdfium.PdfDocument(path)
    assert len(document) == len(reader.pages)
    extracted = []
    for index in range(len(document)):
        page = document[index]
        textpage = page.get_textpage()
        if path.name.startswith("36-puces-"):
            chars = "".join(chr(pdfium_raw.FPDFText_GetUnicode(textpage, i)) for i in range(textpage.count_chars()))
            assert chars.count("Premier élément") == chars.count("Deuxième élément") == chars.count("Troisième élément") == 1
            expected = 24.5 if "interface" in path.name else 17.5 if "125" in path.name else 28.
            ys = [textpage.get_charbox(chars.index(word))[1] for word in ["Premier", "Deuxième", "Troisième"]]
            assert all(abs((ys[i] - ys[i + 1]) - expected) < 0.1 for i in range(2)), (path.name, ys)
            assert chars.count("•") == (1 if "interface" in path.name else 3)
        if path.name.startswith("37-surlignage-partiel-"):
            quads = [list(map(float, a.get_object()["/QuadPoints"])) for a in reader.pages[index].get("/Annots", []) if a.get_object().get("/Subtype") == "/Highlight"]
            assert quads
            def covered(n):
                x0,y0,x1,y1 = textpage.get_charbox(n)
                x,y = (x0+x1)/2,(y0+y1)/2
                for flat in quads:
                    for j in range(0,len(flat),8):
                        q=flat[j:j+8]
                        pts=[(q[k],q[k+1]) for k in [0,2,6,4]]
                        crosses=[(pts[(i+1)%4][0]-pts[i][0])*(y-pts[i][1])-(pts[(i+1)%4][1]-pts[i][1])*(x-pts[i][0]) for i in range(4)]
                        if all(c>=-0.01 for c in crosses) or all(c<=0.01 for c in crosses): return True
                return False
            assert not covered(0) and covered(5), path.name
        if path.name.startswith("35-taille-"):
            chars = "".join(chr(pdfium_raw.FPDFText_GetUnicode(textpage, i)) for i in range(textpage.count_chars()))
            expected = "Texte original à corriger."
            assert chars.count(expected) == 1, (path.name, chars)
            start = chars.index(expected)
            for offset in range(len(expected)):
                size = 13.5 if path.name == "35-taille-bloc.pdf" or offset < 5 else 14.
                assert abs(pdfium_raw.FPDFText_GetFontSize(textpage, start + offset) - size) < 0.01, (path.name, offset)
        if path.name.startswith("34-gras-"):
            bold_names = set()
            for ref in reader.pages[index]['/Resources']['/Font'].values():
                font = ref.get_object()
                descendant = font['/DescendantFonts'][0].get_object() if '/DescendantFonts' in font else font
                descriptor = descendant.get('/FontDescriptor')
                if not descriptor:
                    continue
                descriptor = descriptor.get_object()
                if '/FontFile2' not in descriptor:
                    continue
                program = descriptor['/FontFile2'].get_data()
                # Read the actual embedded TrueType weight. PDFium's weight API
                # defaults to 400 when the optional PDF descriptor weight is absent.
                for offset in range(12, 12 + struct.unpack_from('>H', program, 4)[0] * 16, 16):
                    tag, _, start, length = struct.unpack_from('>4sIII', program, offset)
                    if tag == b'OS/2' and length >= 6 and struct.unpack_from('>H', program, start + 4)[0] >= 600:
                        bold_names.add(str(font['/BaseFont']).lstrip('/'))
            assert bold_names, "The saved PDF must contain an actual bold font program"
            def font_name(char_index):
                buf, flags = create_string_buffer(512), c_int()
                assert pdfium_raw.FPDFText_GetFontInfo(textpage, char_index, buf, len(buf), byref(flags))
                return buf.value.decode('utf-8')
            chars = "".join(chr(pdfium_raw.FPDFText_GetUnicode(textpage, i)) for i in range(textpage.count_chars()))
            expected = "Bloc ajouté à agrandir" if "ajout" in path.name else "Texte original à corriger."
            assert chars.count(expected) == 1, (path.name, chars)
            start = chars.index(expected)
            for char_index in range(start, start + len(expected)):
                assert abs(pdfium_raw.FPDFText_GetFontSize(textpage, char_index) - 14.) < 0.01, "Resize changed the font size"
                assert font_name(char_index) in bold_names, "Resize lost bold"
            if "ajout" in path.name:
                assert chars.count("Voisin intact") == 1
                neighbour = chars.index("Voisin intact")
                assert abs(pdfium_raw.FPDFText_GetFontSize(textpage, neighbour) - 14.) < 0.01
                assert font_name(neighbour) not in bold_names
        if path.name == "20-bloc-copie.pdf" and index == 0:
            chars = "".join(chr(pdfium_raw.FPDFText_GetUnicode(textpage, i)) for i in range(textpage.count_chars()))
            for word, expected in [("français", (204, 26, 51)), ("avec", (0, 0, 0)), ("apostrophe", (0, 0, 0))]:
                values = [c_uint() for _ in range(4)]
                assert pdfium_raw.FPDFText_GetFillColor(textpage, chars.index(word), *(byref(v) for v in values))
                assert all(abs(v.value - e) <= 1 for v, e in zip(values, expected)), f"Colour leaked into {word}"
        extracted.append(textpage.get_text_range())
        bitmap = page.render(scale=1.0)
        assert bitmap.width > 0 and bitmap.height > 0
        if index == 0 and (path.name.startswith(("34-gras-", "35-taille-")) or path.name in {"02-correction.pdf", "05-image.pdf", "06-surlignage.pdf", "19-ocr-windows.pdf", "22-traits-tableau.pdf", "22-formes-0.pdf", "23-ocr-regions-ajout-90.pdf", "24-styles-0.pdf", "24-style-origine.pdf"}):
            bitmap.to_pil().save(path.with_suffix(".pdfium.png"))
        bitmap.close()
        textpage.close()
        page.close()
    text = "\n".join(extracted)
    compact = " ".join(text.split())
    if path.name == "26-bloc-supprime.pdf":
        assert not compact, "The deleted source text must not remain extractable"
    if path.name == "26-cadre-trace.pdf":
        assert compact.count("Été français") == 1 and "œ, ç et à" in compact
        assert compact.count("Texte original") == 1
    if path.name == "20-bloc-copie.pdf":
        assert compact.count("Un été français") == 1 and "Destination à conserver" in compact
        assert not reader.pages[0].get("/Annots")
    if path.name == "20-bloc-copie-corrige.pdf":
        assert compact.count("corrigé après collage") == 1
    if path.name.startswith("20-ajout-colle-"):
        assert compact.count("Été : é è ê à ç œ") == 1
    if path.name == "21-pdf-concatenes.pdf":
        assert len(document) == 4
        assert "principal" in extracted[0] and "Premier document" in extracted[1]
        assert "Second document page 1" in extracted[2] and "Second document page 2" in extracted[3]
        assert reader.pages[1].get("/Rotate") == 90
        annots = [a.get_object() for a in reader.pages[1]["/Annots"]]
        assert any(a.get("/Subtype") == "/Highlight" for a in annots)
        assert any(a.get("/Contents") == "Commentaire conservé" for a in annots)
        images = reader.pages[1]["/Resources"]["/XObject"].get_object()
        assert any("/SMask" in obj.get_object() for obj in images.values())
    if path.name == "21-pdf-concatenes-ordonnes.pdf":
        assert len(document) == 4 and "Second document page 2" in extracted[0]
    if path.name.startswith("18-ocr-"):
        assert text.count("Texte corrigé") == 1 and "œ" in text and "l’été" in text
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert not any(op == b"Do" for _, op in operations), "Converted image must not remain painted behind the editable text"
    if path.name == "19-ocr-windows.pdf":
        assert text.count("puis corrigé") == 1
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert sum(op == b"Do" for _, op in operations) == 1, "Only the cleaned raster background must remain"
        assert compact.count("École française") == 1
    if path.name.startswith("23-ocr-regions-"):
        assert len(document) == 2 and "Autre page intacte" in extracted[1]
        assert "5 cm" in extracted[0] and extracted[0].count("Été") == 1
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert sum(op == b"Do" for _, op in operations) == 1
        assert sum(op == b"BT" for _, op in operations) == 4
        assert any(a.get_object().get("/Contents") == "Note à conserver" for a in reader.pages[0]["/Annots"])
    if path.name.startswith("22-"):
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert not any(op == b"Do" for _, op in operations), "Drawings must be native vectors"
        assert text.count("reste intact") == 1
        strokes = sum(op == b"S" for _, op in operations)
        if path.name == "22-traits-tableau.pdf":
            assert strokes == 6, "No old stroke may remain underneath its replacement"
            assert any(op == b"w" and float(args[0]) == 3 for args, op in operations)
        elif path.name == "22-tableau-separe.pdf":
            assert strokes == 9
        elif path.name == "22-trait-transforme.pdf":
            assert strokes == 1
        else:
            assert strokes == 4
            assert sum(op == b"c" for _, op in operations) == 4
    if path.name.startswith("24-styles-"):
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        patterns = [(list(map(float,args[0])),float(args[1])) for args,op in operations if op == b"d"]
        expected = [([],0),([15,9],0),([0,9],0),([18,7.5,0,7.5],0),([27,9],0),([],0)]
        assert len(patterns) == len(expected)
        assert all(len(actual[0]) == len(wanted[0]) and abs(actual[1]-wanted[1]) < 0.002
                   and all(abs(a-b) < 0.002 for a,b in zip(actual[0],wanted[0]))
                   for actual,wanted in zip(patterns,expected))
        assert sum(op == b"S" for _,op in operations) == 6
        assert sum(op == b"m" for _,op in operations) == 7, "Double is two parallel native subpaths"
        assert [float(args[0]) for args,op in operations if op == b"J"] == [0,0,1,1,0,0]
        assert [float(args[0]) for args,op in operations if op == b"w"][-1] == 1
        assert not any(op == b"Do" for _,op in operations)
    if path.name == "24-style-origine.pdf":
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert any(op == b"d" and list(map(float,args[0])) == [8,4,2,4] and float(args[1]) == 2 for args,op in operations)
        assert sum(op == b"S" for _,op in operations) == 1
    if path.name.startswith("25-page-blanche-"):
        assert len(document) == 3
        assert "Page initiale 1" in extracted[0] and "Page initiale 2" in extracted[2]
        assert not extracted[1].strip() and reader.pages[1].get_contents() is None
        first = document[0];blank = document[1]
        assert first.get_size() == blank.get_size()
        first.close();blank.close()
    if path.name == "02-correction.pdf":
        assert text.count("corrigée") == 1 and "originale" not in text
    if path.name == "03-deplacement.pdf" or path.name.startswith("10-rotation-"):
        assert text.count("originale") == 1
    if path.name == "04-francais.pdf":
        assert "é è ê à ç œ l’été aujourd'hui" in text
    if path.name == "05-image.pdf":
        images = reader.pages[0]["/Resources"]["/XObject"].get_object()
        assert any("/SMask" in obj.get_object() for obj in images.values())
    if path.name == "06-surlignage.pdf":
        annotations = [a.get_object() for a in reader.pages[0]["/Annots"]]
        assert any(a.get("/Subtype") == "/Highlight" and a.get("/QuadPoints") for a in annotations)
    if path.name == "11-pages-organisees.pdf":
        assert len(document) == 3
        assert all(f"page {identity}" in extracted[page] for page, identity in enumerate([2, 3, 1]))
        assert reader.pages[2].get("/Rotate") == 90
        assert any(a.get_object().get("/Subtype") == "/Highlight" for a in reader.pages[2]["/Annots"])
        images = reader.pages[2]["/Resources"]["/XObject"].get_object()
        assert any("/SMask" in obj.get_object() for obj in images.values())
    if path.name == "11-pages-reeditees.pdf":
        assert "corrigée" in extracted[2]
    if path.name.startswith("12-formatage"):
        assert text.count("phrase") == 1
        operations = ContentStream(reader.pages[0].get_contents(), reader).operations
        assert sum(op == b"S" for _, op in operations) == 2, "Old decorations must not accumulate"
        if "reedite" in path.name:
            assert "corrigée" in text and "originale" not in text
    if path.name == "13-exposant-indice.pdf":
        rises = [float(args[0]) for args,op in ContentStream(reader.pages[0].get_contents(),reader).operations if op == b"Ts"]
        assert any(v>0 for v in rises) and any(v<0 for v in rises)
    if path.name.startswith("14-police") or path.name in {"16-police-arial.pdf","17-police-adaptee.pdf"}:
        fonts = reader.pages[0]["/Resources"]["/Font"].get_object()
        assert any("/DescendantFonts" in f.get_object() and "/FontFile2" in f.get_object()["/DescendantFonts"][0].get_object()["/FontDescriptor"].get_object() for f in fonts.values())
        if path.name.startswith("14-police"):
            assert "œ" in text and "l’été" in text
    if path.name == "15-commentaire.pdf":
        annotations = [a.get_object() for a in reader.pages[0]["/Annots"]]
        assert any(a.get("/Subtype") == "/Text" and a.get("/Contents") == "Vérifier cette phrase, s’il vous plaît." for a in annotations)
    results.append({"file": path.name, "pages": len(document), "pdfium_render": True,
                    "strict_parse": True, "text": text})
    document.close()

assert len(results) >= 14, "Run Rust roundtrip tests first"
(ROOT / "test-output/independent-results.json").write_text(
    json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8"
)
print(f"{len(results)} PDF validés par PDFium (rendu/extraction) et pypdf (lecture stricte).")
