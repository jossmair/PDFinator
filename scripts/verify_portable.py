"""Verify the delivered two-file ZIP and the extracted native application."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
archive = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / f"dist/PDFinator-{version}-portable-exe-windows-x64.zip"
expected = {"PDFinator/PDFinator.exe": ROOT / "runtime/pdfinator.exe",
            "PDFinator/LICENCES.txt": ROOT / "licenses/LICENCES.txt"}
sha = lambda data: hashlib.sha256(data).hexdigest()
test_root = (ROOT / "test-output").resolve()
test_root.mkdir(exist_ok=True)
with zipfile.ZipFile(archive) as z:
    assert len(z.infolist()) == 2 and set(z.namelist()) == set(expected)
    assert z.testzip() is None
    for entry, source in expected.items():
        assert z.read(entry) == source.read_bytes(), entry
    with tempfile.TemporaryDirectory(prefix="portable extrait-é ", dir=test_root) as temporary:
        assert Path(temporary).resolve().is_relative_to(test_root)
        z.extractall(temporary)
        executable = Path(temporary) / "PDFinator/PDFinator.exe"
        data = executable.read_bytes()
        assert data[:2] == b"MZ"
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        assert data[pe:pe+4] == b"PE\0\0"
        assert struct.unpack_from("<H", data, pe + 4)[0] == 0x8664
        assert struct.unpack_from("<H", data, pe + 24 + 68)[0] == 2
        subprocess.run([sys.executable, str(ROOT / "scripts/verify_executable_icon.py"), str(executable)], check=True)
        powershell = shutil.which('pwsh') or str(Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe')
        subprocess.run([powershell, '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                        str(ROOT / 'scripts/verify-shell-icon.ps1'), '-Executable', str(executable)], check=True)
result = {"version": version, "archive": str(archive), "sha256": sha(archive.read_bytes()),
          "files": list(expected), "bytes": archive.stat().st_size,
          "crc_valid": True, "payload_matches": True, "extracted_x64_gui_exe": True,
          "icon_verified": True, "executable_sha256": sha((ROOT / "runtime/pdfinator.exe").read_bytes())}
(ROOT / f"portable-exe-verification-{version.replace('.', '')}.json").write_text(
    json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
print(json.dumps(result, ensure_ascii=False, indent=2))
