"""Generate two redistributable minimal Office regression PDFs (development only)."""
import argparse
from pathlib import Path
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
args.directory.mkdir(parents=True, exist_ok=True)
# The existing openly licensed Inter face is the only embedded asset.
pdfmetrics.registerFont(TTFont("InterFractionProbe", str(root / "vendor/pdfcraft/assets/fonts/Inter-Regular.ttf")))
for name, numerator in [("positive", "12"), ("unicode-minus", "−12")]:
    c = canvas.Canvas(str(args.directory / f"{name}.pdf"), pagesize=(200, 200), invariant=1)
    c.setFont("InterFractionProbe", 12)
    c.drawString(80, 160, numerator)
    c.line(78, 155, 112, 155)
    c.drawString(87, 140, "7")
    c.save()
print("Two minimal fraction PDFs generated.")
