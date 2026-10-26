"""Generate redistributable synthetic PDFs for Office rendering regressions.

Development dependency: reportlab. No user documents or machine fonts required.
"""
from pathlib import Path
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parents[1]
out = ROOT / "test-output/office-corpus"
out.mkdir(parents=True, exist_ok=True)
c = canvas.Canvas(str(out / "scripts.pdf"), pagesize=(595, 842), invariant=1)
c.setFont("Helvetica-Bold", 18)
c.drawString(50, 780, "Indices et exposants")
for y, lowered in [(700, True), (640, False)]:
    text = c.beginText(50, y)
    text.setFont("Helvetica", 14)
    text.textOut("H" if lowered else "x")
    text.setFont("Helvetica", 9)
    text.setRise(-4 if lowered else 6)
    text.textOut("2")
    text.setRise(0)
    text.setFont("Helvetica", 14)
    text.textOut("O : eau" if lowered else " + y = 10")
    c.drawText(text)
c.save()

c = canvas.Canvas(str(out / "mixed-pages.pdf"), pagesize=(595, 842), invariant=1)
for size, label in [((595, 842), "Premiere page"), ((842, 595), None),
                    ((420, 595), "Derniere page")]:
    c.setPageSize(size)
    if label:
        c.setFont("Helvetica", 12)
        c.drawString(50, size[1] - 70, label)
    c.showPage()
c.save()
print("Synthetic Office corpus generated: scripts.pdf, mixed-pages.pdf")
