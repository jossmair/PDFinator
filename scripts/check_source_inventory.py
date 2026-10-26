"""Check public legal provenance without printing a builder's local paths."""
from pathlib import Path
import json
import re

root = Path(__file__).resolve().parents[1]
inventory = json.loads((root / "licenses/portable-inventory.json").read_text(encoding="utf-8"))
sources = [source for item in inventory["documents"].values() for source in item["sources"]]
assert sources, "No legal source references found"
assert all(not Path(source).is_absolute() and not re.match(r"^[A-Za-z]:[\\/]", source)
           and not source.startswith("\\\\") for source in sources), "Local absolute paths must not be published"
print(f"Public licence inventory checked: {len(sources)} relative source references.")
