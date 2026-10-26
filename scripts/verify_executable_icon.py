"""Development-only verification of the actual Windows PE icon resources."""
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[1]
exe = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "runtime/pdfinator.exe"
data = exe.read_bytes()
u16 = lambda offset: struct.unpack_from("<H", data, offset)[0]
u32 = lambda offset: struct.unpack_from("<I", data, offset)[0]
pe = u32(0x3C)
assert data[pe:pe + 4] == b"PE\0\0"
optional = pe + 24
assert u16(optional) == 0x20B, "Expected a Windows x64 executable"
resource_rva = u32(optional + 112 + 16)
sections = optional + u16(pe + 20)

def file_offset(rva):
    for index in range(u16(pe + 6)):
        section = sections + index * 40
        virtual, address, size, raw = struct.unpack_from("<IIII", data, section + 8)
        if address <= rva < address + max(virtual, size):
            return raw + rva - address
    raise ValueError(f"Unmapped resource RVA: {rva}")

base = file_offset(resource_rva)

def children(relative):
    directory = base + relative
    count = u16(directory + 12) + u16(directory + 14)
    return [struct.unpack_from("<II", data, directory + 16 + index * 8) for index in range(count)]

def payloads(relative, depth=0):
    assert depth < 4
    found = []
    for _, target in children(relative):
        if target & 0x80000000:
            found.extend(payloads(target & 0x7FFFFFFF, depth + 1))
        else:
            rva, size = struct.unpack_from("<II", data, base + target)
            offset = file_offset(rva)
            found.append(data[offset:offset + size])
    return found

types = dict(children(0))
assert 3 in types and 14 in types, "Missing executable icon/group resource"
actual = payloads(types[3] & 0x7FFFFFFF)
groups = payloads(types[14] & 0x7FFFFFFF)
assert len(groups) == 1
ico = (root / "assets/pdfinator.ico").read_bytes()
count = struct.unpack_from("<H", ico, 4)[0]
expected = []
sizes = []
for index in range(count):
    entry = 6 + index * 16
    size, offset = struct.unpack_from("<II", ico, entry + 8)
    expected.append(ico[offset:offset + size])
    sizes.append(ico[entry] or 256)
assert struct.unpack_from("<H", groups[0], 4)[0] == count
assert sorted(actual) == sorted(expected), "Embedded EXE icon differs from the application's ICO"
assert sizes == [16, 20, 24, 32, 40, 48, 64, 128, 256]
for size, frame in zip(sizes, expected):
    if size < 256:
        assert struct.unpack_from('<IIIHH', frame) == (40, size, size*2, 1, 32), 'Expected a shell-compatible 32-bit DIB'
    else:
        assert frame.startswith(b'\x89PNG\r\n\x1a\n')
print(f"Icone EXE verifiee : {count} definitions, de 16 a 256 pixels, identiques au fichier ICO.")
