"""Independent Office package checks. Development only; never shipped."""
from pathlib import Path, PurePosixPath
from zipfile import ZipFile, ZIP_STORED
import json
import sys
import xml.etree.ElementTree as ET
from docx import Document

ROOT = Path(__file__).resolve().parents[1]
NS = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
      "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
      "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
      "table": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
      "draw": "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0",
      "xlink": "http://www.w3.org/1999/xlink"}
NS["wp"] = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
NS["m"] = "http://schemas.openxmlformats.org/officeDocument/2006/math"
results = []
for path in sorted((ROOT / "test-output").glob("38-*")):
    if path.suffix not in {".docx", ".odt"}:
        continue
    with ZipFile(path) as z:
        assert z.testzip() is None
        for name in z.namelist():
            if name.endswith(".xml") or name.endswith(".rels"):
                ET.fromstring(z.read(name))
        if path.suffix == ".docx":
            doc = Document(path)
            xml = z.read("word/document.xml")
            root = ET.fromstring(xml)
            assert b"txbxContent" not in xml and b"v:shape" not in xml
            for anchor in root.findall(".//wp:anchor", NS):
                assert anchor.attrib["behindDoc"] == "1"  # ornamental paint only
            all_text = "".join(root.itertext())
            tables = root.findall(".//w:tbl", NS)
            pictures = root.findall(".//w:drawing", NS)
            relationships = ET.fromstring(z.read("word/_rels/document.xml.rels"))
            for rel in relationships:
                assert "TargetMode" not in rel.attrib
                assert str(PurePosixPath("word") / rel.attrib["Target"]) in z.namelist()
            if "structure" in path.stem:
                assert len(doc.tables) == 1
                assert len(doc.tables[0].rows) == 4 and len(doc.tables[0].columns) == 3
                assert len(pictures) == 1
                assert len(root.findall(".//w:shd", NS)) == 3
                for word in ["Alice", "Benoît", "Chloé", "Très bien"]:
                    assert all_text.count(word) == 1
            if "reel" in path.stem:
                assert len(doc.sections) == 2
                assert root.findall(".//w:gridSpan", NS) and root.findall(".//w:vMerge", NS)
                assert len(pictures) >= 2
                assert "Sujet 2" in all_text
                assert all_text.count("Exercice") == 5
                assert len(root.findall(".//m:acc", NS)) == 2
                fonts = ET.fromstring(z.read("word/fontTable.xml"))
                assert len(fonts) > 0, "The missing source face must remain embedded"
                for face in fonts:
                    for embedded in face:
                        if embedded.tag.endswith("embedRegular"):
                            assert embedded.attrib[f"{{{NS['w']}}}subsetted"] == "1"
        else:
            assert z.infolist()[0].filename == "mimetype"
            assert z.infolist()[0].compress_type == ZIP_STORED
            assert z.read("mimetype") == b"application/vnd.oasis.opendocument.text"
            root = ET.fromstring(z.read("content.xml"))
            assert root.findall(".//text:p", NS)
            assert not root.findall(".//draw:text-box", NS)
            all_text = "".join(root.itertext())
            tables = root.findall(".//table:table", NS)
            pictures = root.findall(".//draw:image", NS)
            for picture in pictures:
                assert picture.attrib[f"{{{NS['xlink']}}}href"] in z.namelist()
            if "structure" in path.stem:
                assert len(tables) == 1 and len(pictures) == 1
                assert len(tables[0].findall("table:table-row", NS)) == 4
                for word in ["Alice", "Benoît", "Chloé", "Très bien"]:
                    assert "".join(all_text.split()).count("".join(word.split())) == 1, (path.name, word)
            if "reel" in path.stem:
                assert len(pictures) >= 2 and all_text.count("Exercice") == 5
        results.append({"file": path.name, "tables": len(tables), "images": len(pictures), "text_boxes": 0, "bytes": path.stat().st_size})
assert len(results) >= 5
(ROOT / "test-output/office-results.json").write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
print(f"{len(results)} fichiers DOCX/ODT validés : paragraphes, cellules, images, styles et archives.")
rendered = []
sys.path.insert(0, str(ROOT / ".tools/oracle"))
from pypdf import PdfReader
for directory, name, expected in [
    ("office-0510-old-docx", "38-document-reel", 2),
    ("office-0510-old-odt", "38-document-reel", 2),
    ("office-0510-structure-docx", "38-structure", 1),
    ("office-0510-structure-odt", "38-structure", 1),
]:
    path = ROOT / "test-output" / directory / f"{name}.pdf"
    if not path.exists():
        continue
    reader = PdfReader(path, strict=True)
    assert len(reader.pages) == expected, (directory, len(reader.pages))
    content = "".join("".join((page.extract_text() or "").split()) for page in reader.pages)
    required = [("Cassandra", 2), ("Exercice 5", 1), ("Calculs", 1)] if "reel" in name else [(word, 1) for word in ["Compte rendu de la classe", "Alice", "Chloé", "Les résultats"]]
    for word, count in required:
        assert content.count("".join(word.split())) == count, (directory, word)
    for index in range(expected):
        assert path.with_name(f"page-{index + 1}.png").is_file()
    rendered.append({"directory": directory, "pages": len(reader.pages), "required_text_counts": True})
(ROOT / "test-output/office-render-results.json").write_text(json.dumps(rendered, indent=2), encoding="utf-8")
if rendered:
    print(f"{sum(r['pages'] for r in rendered)} pages rendues relues, texte conservé et pagination contrôlée.")
