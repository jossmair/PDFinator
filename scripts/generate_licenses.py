"""Development-only legal bundle generator. No dependency or network at runtime.

Walk the Windows normal dependency graph (not every package in Cargo.lock),
retain actual licence/notice texts, and coalesce identical documents. Missing
licences use checked-in, pinned upstream texts rather than guessed notices.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EXTRA = ROOT / "licenses/extra"
OUTPUT = ROOT / "licenses/LICENCES.txt"
INVENTORY = ROOT / "licenses/portable-inventory.json"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(text: str) -> str:
    return re.sub(r"\s+", " ", text.strip())


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig").replace("\r\n", "\n").strip()


def metadata() -> dict:
    env = os.environ.copy()
    cargo = ROOT / ".tools/cargo/bin/cargo.exe"
    if cargo.is_file():
        env["CARGO_HOME"] = str(ROOT / ".tools/cargo")
        env["RUSTUP_HOME"] = str(ROOT / ".tools/rustup")
    else:
        cargo = "cargo"
    return json.loads(subprocess.check_output([
        str(cargo), "metadata", "--locked", "--offline", "--filter-platform",
        "x86_64-pc-windows-msvc", "--format-version", "1",
    ], cwd=ROOT, env=env))


def normal_packages(meta: dict) -> list[dict]:
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    packages = {package["id"]: package for package in meta["packages"]}
    pending = [meta["resolve"]["root"]]
    seen = set()
    while pending:
        key = pending.pop()
        if key in seen:
            continue
        seen.add(key)
        for dep in nodes[key]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    return sorted((packages[key] for key in seen), key=lambda p: (p["name"], p["version"]))


def font_notices(path: Path) -> dict[int, str]:
    """Read the font's own copyright and licence records without a font library."""
    data = path.read_bytes()
    tables = struct.unpack_from(">H", data, 4)[0]
    offset = next(struct.unpack_from(">I", data, 12 + i * 16 + 8)[0]
                  for i in range(tables) if data[12+i*16:16+i*16] == b"name")
    count, strings = struct.unpack_from(">HH", data, offset + 2)
    values: dict[int, list[str]] = {}
    for index in range(count):
        platform, _, _, name, size, start = struct.unpack_from(">HHHHHH", data, offset + 6 + index * 12)
        if name not in (0, 13):
            continue
        encoding = "utf-16-be" if platform in (0, 3) else "mac_roman"
        text = data[offset+strings+start:offset+strings+start+size].decode(encoding).strip()
        if text not in values.setdefault(name, []):
            values[name].append(text)
    return {name: "\n\n".join(texts) for name, texts in values.items()}


def licence_files(package: dict) -> list[Path]:
    directory = Path(package["manifest_path"]).parent
    name, version = package["name"], package["version"]
    if name.startswith("pdfcraft-"):
        return [ROOT / "vendor/pdfcraft/LICENSE-MIT"]
    if name == "craft-print":
        return [ROOT / "vendor/craft-libs/LICENSE-MIT"]
    files = [p for p in directory.iterdir() if p.is_file() and re.match(
        r"^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|OFL|UNLICENSE)", p.name, re.I)]
    if package.get("license_file"):
        path = directory / package["license_file"]
        if path.is_file() and path not in files:
            files.append(path)
    for folder in ("LICENSES", "licenses"):
        nested = directory / folder
        if nested.is_dir():
            files.extend(p for p in nested.rglob("*") if p.is_file())
    expression = package.get("license") or ""
    mit = [p for p in files if re.search(r"(^MIT[.]|LICENSE[-_]MIT|LICENCE[-_]MIT)", p.name, re.I)]
    apache = [p for p in files if re.search(r"(^Apache-2[.]|LICENSE[-_]APACHE)", p.name, re.I)]
    # Select a permissive alternative explicitly, retaining AND supplements and
    # separate COPYRIGHT/NOTICE texts. Never discard an AND requirement.
    if mit and "MIT" in expression and ("AND" not in expression or "OR" in expression):
        supplements = [p for p in files if p not in mit and re.match(r"^(COPYRIGHT|NOTICE)", p.name, re.I)]
        if "AND" in expression:
            supplements += [p for p in files if p not in mit and p not in apache
                            and not re.search(r"ZLIB|UNLICENSE|0BSD", p.name, re.I)]
        return sorted(set(mit + supplements))
    if apache and "OR" in expression and "AND" not in expression:
        return sorted(set(apache + [p for p in files if re.match(r"^(COPYRIGHT|NOTICE)", p.name, re.I)]))
    if files:
        return sorted(set(files))
    fallback = {
        "accesskit": ("0.24.1", "accesskit-MIT.txt"),
        "accesskit_consumer": ("0.35.0", "accesskit-MIT.txt"),
        "accesskit_windows": ("0.32.1", "accesskit-MIT.txt"),
        "alloc-stdlib": ("0.2.4", "alloc-stdlib-BSD.txt"),
        "clipboard-win": ("5.4.1", "clipboard-win-BSL.txt"),
        "profiling": ("1.0.18", "profiling-MIT.txt"),
        "fmt-derive": ("0.1.2", "accesskit-MIT.txt"),
        "fmt-derive-proc": ("0.1.2", "accesskit-MIT.txt"),
    }
    if name == "accesskit_winit" and version == "0.32.2":
        return [ROOT / "licenses/LICENSE-APACHE"]
    if name in {"ecolor", "eframe", "egui", "egui-winit", "egui_extras", "egui_glow", "emath", "epaint", "epaint_default_fonts"} and version == "0.36.2":
        return [EXTRA / "egui-MIT.txt"]
    if name in fallback and fallback[name][0] == version:
        return [EXTRA / fallback[name][1]]
    raise RuntimeError(f"Missing reviewed licence text for {name} {version}")


def make_bundle(meta: dict, executable: bytes) -> tuple[str, dict]:
    packages = normal_packages(meta)
    documents: dict[str, dict] = {}
    records = []

    def add(origin: str, text: str, source: str) -> str:
        if not text.strip():
            raise RuntimeError(f"Empty legal text: {source}")
        key = digest(canonical(text).encode("utf-8"))
        item = documents.setdefault(key, {"text": text.strip(), "origins": set(), "sources": set()})
        item["origins"].add(origin)
        item["sources"].add(source)
        return key

    def add_file(origin: str, path: Path) -> str:
        return add(origin, read(path), str(path))

    for package in packages:
        name = package["name"]
        if name == "pdfinator":
            text = "PDFinator — Ergonatus\nApplication et pictogrammes originaux, 2026.\n\n" + read(EXTRA / "accesskit-MIT.txt")
            keys = [add("PDFinator — Ergonatus", text, "Cargo.toml; licenses/PDFINATOR-NOTICE.txt")]
        else:
            origin = "Moteur documentaire PDF" if name.startswith("pdfcraft-") else f"{name} {package['version']}"
            keys = [add_file(origin, file) for file in licence_files(package)]
            # Some published crates omit a copyright line from their licence
            # file; retain their declared authors alongside the complete text.
            if all("copyright" not in documents[key]["text"].lower().split("permission")[0] for key in keys) and package.get("authors"):
                keys.append(add(origin, "Authors: " + "; ".join(package["authors"]), package["manifest_path"]))
        records.append({"package": name, "version": package["version"], "declared": package["license"], "documents": keys})

    by_name = {p["name"]: p for p in packages}
    cmap = Path(by_name["hayro-cmap"]["manifest_path"]).parent / "assets/LICENSE.txt"
    add_file("Ressources CMap Adobe", cmap)
    foxit = ROOT / "vendor/pdfcraft/vendor/hayro-interpret/assets"
    if any(font.read_bytes() in executable for font in foxit.glob("*.pfb")):
        add_file("Polices de substitution PDF standard", foxit / "LICENSE_FOXIT")

    # The raw font bytes are embedded by include_bytes!. The final executable
    # identifies which families survive linking, excluding unused source assets.
    embedded = []
    font_root = ROOT / "vendor/pdfcraft/assets/fonts"
    for family, names in {"Inter": ["Inter-Regular.ttf", "Inter-SemiBold.ttf", "Inter-Medium.ttf"],
                          "DancingScript": ["DancingScript.ttf"],
                          "JetBrainsMono": ["JetBrainsMono-Regular.ttf"]}.items():
        used = [name for name in names if (font_root / name).read_bytes() in executable]
        if used:
            add_file(f"Police {family}", font_root / f"OFL-{family}.txt")
            embedded.extend(used)
    default_fonts = Path(by_name["epaint_default_fonts"]["manifest_path"]).parent / "fonts"
    ofl = read(font_root / "OFL-Inter.txt")
    ofl = ofl[ofl.index("SIL OPEN FONT LICENSE"):]
    for font in sorted(default_fonts.glob("*.ttf")):
        if font.read_bytes() not in executable:
            continue
        embedded.append(font.name)
        values = font_notices(font)
        if font.name == "emoji-icon-font.ttf":
            add_file("Police emoji-icon-font", default_fonts / "emoji-icon-font-mit-license.txt")
        elif font.name == "Ubuntu-Light.ttf":
            add("Police Ubuntu Light", values[0] + "\n\n" + read(EXTRA / "Ubuntu-font-1.0.txt"), str(font))
        elif font.name == "NotoEmoji-Regular.ttf":
            add("Police Noto Emoji", values[0] + "\n\n" + values[13] + "\n\n" + ofl, str(font))
        elif font.name == "Hack-Regular.ttf":
            add("Police Hack", values[13], str(font))
        else:
            raise RuntimeError(f"Unreviewed default font: {font}")

    sections = ["PDFinator — Ergonatus\nMentions légales des composants et polices intégrés.\n"
                "Les textes identiques sont regroupés ; les droits et avis d’auteur sont conservés.\n"]
    for item in documents.values():
        sections.append("=" * 72 + "\n" + ", ".join(sorted(item["origins"])) + "\n\n" + item["text"] + "\n")
    bundle = "\n".join(sections)
    inventory = {
        "target": "x86_64-pc-windows-msvc", "executable_sha256": digest(executable),
        "cargo_lock_sha256": digest((ROOT / "Cargo.lock").read_bytes()),
        "normal_packages": len(packages), "unique_documents": len(documents),
        "embedded_fonts": embedded, "packages": records,
        "documents": {key: {"origins": sorted(value["origins"]), "sources": sorted(value["sources"])}
                      for key, value in documents.items()},
        "bundle_sha256": digest(bundle.encode("utf-8")),
    }
    return bundle, inventory


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    sources = json.loads((EXTRA / "SOURCES.json").read_text(encoding="utf-8"))
    for source in sources:
        assert digest((EXTRA / source["file"]).read_bytes()) == source["sha256"], source["file"]
    executable = (ROOT / "runtime/pdfinator.exe").read_bytes()
    text, inventory = make_bundle(metadata(), executable)
    if args.check:
        assert OUTPUT.read_bytes() == text.encode("utf-8"), "Regenerate LICENCES.txt for this build"
        assert json.loads(INVENTORY.read_text(encoding="utf-8")) == inventory, "Stale legal inventory"
    else:
        OUTPUT.write_bytes(text.encode("utf-8"))
        INVENTORY.write_text(json.dumps(inventory, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"{inventory['normal_packages']} composants Windows, {inventory['unique_documents']} textes uniques, "
          f"{len(text.encode('utf-8'))} octets dans LICENCES.txt.")


if __name__ == "__main__":
    main()
