//! Preserve source font versions as embedded document content. A locally
//! installed face with the same name may have different glyph metrics.
use super::*;
fn assign(
    run: &mut Run,
    face: &ttf_parser::Face<'_>,
    family: &str,
    alias: &str,
    bold: bool,
    italic: bool,
) -> bool {
    let mut used = false;
    if let Some(f) = &mut run.fraction {
        for r in f.numerator.iter_mut().chain(&mut f.denominator) {
            used |= assign(r, face, family, alias, bold, italic);
        }
    }
    if run.font == family
        && run.bold == bold
        && run.italic == italic
        && run.text.chars().all(|c| {
            c.is_whitespace()
                || (run.fraction.is_some() && c == '/')
                || face.glyph_index(c).is_some()
        })
    {
        run.font = alias.into();
        used = true;
    }
    used
}
// sfnt table checksums and checksumAdjustment must be rebuilt after assigning
// a unique family name: otherwise Writer may substitute a different face.
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, b| {
        let mut word = [0; 4];
        word[..b.len()].copy_from_slice(b);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}
#[cfg(test)]
fn rename(bytes: &[u8], family: &str) -> Option<Vec<u8>> {
    let f = ttf_parser::Face::parse(bytes, 0).ok()?;
    rename_styled(bytes, family, f.is_bold(), f.is_italic())
}
fn rename_styled(bytes: &[u8], family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
    let u16_at = |i| {
        bytes
            .get(i..i + 2)
            .map(|v| u16::from_be_bytes(v.try_into().unwrap()))
    };
    let u32_at = |i| {
        bytes
            .get(i..i + 4)
            .map(|v| u32::from_be_bytes(v.try_into().unwrap()))
    };
    let count = usize::from(u16_at(4)?);
    if count > 128 {
        return None;
    }
    let mut tables = std::collections::BTreeMap::new();
    for i in 0..count {
        let at = 12 + i * 16;
        let tag: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        let off = u32_at(at + 8)? as usize;
        let len = u32_at(at + 12)? as usize;
        tables.insert(tag, bytes.get(off..off.checked_add(len)?)?.to_vec());
    }
    // Some PDF subsets only have a format-12 Unicode cmap. Include the BMP
    // format-4 map required by older Office font loaders as well.
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let glyphs: Vec<_> = (0u32..0xffff)
        .filter_map(|c| {
            let ch = char::from_u32(c)?;
            let glyph = face.glyph_index(ch)?;
            (glyph.0 != 0).then_some((c as u16, glyph.0))
        })
        .take(4096)
        .collect();
    if glyphs.len() < 4096 {
        let n = glyphs.len() + 1;
        let power = 1u16 << (n as u16).ilog2();
        let mut bmp = Vec::new();
        for v in [
            4u16,
            (16 + n * 8) as u16,
            0,
            n as u16 * 2,
            power * 2,
            power.ilog2() as u16,
            (n as u16 - power) * 2,
        ] {
            bmp.extend(v.to_be_bytes());
        }
        for (c, _) in &glyphs {
            bmp.extend(c.to_be_bytes());
        }
        bmp.extend(0xffffu16.to_be_bytes());
        bmp.extend(0u16.to_be_bytes());
        for (c, _) in &glyphs {
            bmp.extend(c.to_be_bytes());
        }
        bmp.extend(0xffffu16.to_be_bytes());
        for (c, g) in &glyphs {
            bmp.extend(g.wrapping_sub(*c).to_be_bytes());
        }
        bmp.extend(1u16.to_be_bytes());
        bmp.resize(16 + n * 8, 0);
        let cmap = tables.get(b"cmap")?;
        let old_count = u16::from_be_bytes(cmap.get(2..4)?.try_into().ok()?) as usize;
        let old_header = 4 + old_count * 8;
        let header = old_header + 8;
        let mut unicode = vec![0, 0];
        unicode.extend(((old_count + 1) as u16).to_be_bytes());
        unicode.extend(3u16.to_be_bytes());
        unicode.extend(1u16.to_be_bytes());
        unicode.extend((header as u32).to_be_bytes());
        for i in 0..old_count {
            let at = 4 + i * 8;
            unicode.extend(cmap.get(at..at + 4)?);
            let off = u32::from_be_bytes(cmap.get(at + 4..at + 8)?.try_into().ok()?) as usize;
            unicode.extend(((off + 8 + bmp.len()) as u32).to_be_bytes());
        }
        unicode.extend(bmp);
        unicode.extend(cmap.get(old_header..)?);
        tables.insert(*b"cmap", unicode);
    }
    let mut records = Vec::new();
    let mut strings = Vec::new();
    if let Some(original) = tables.get(b"name") {
        let at = |i| {
            original
                .get(i..i + 2)
                .map(|v| u16::from_be_bytes(v.try_into().unwrap()))
        };
        let number = at(2)? as usize;
        let offset = at(4)? as usize;
        for i in 0..number {
            let p = 6 + i * 12;
            let id = at(p + 6)?;
            if matches!(id, 1 | 2 | 4 | 6 | 16 | 17) {
                continue;
            }
            let len = at(p + 8)? as usize;
            let start = offset + at(p + 10)? as usize;
            let data = original.get(start..start.checked_add(len)?)?;
            records.extend_from_slice(original.get(p..p + 8)?);
            records.extend((len as u16).to_be_bytes());
            records.extend((strings.len() as u16).to_be_bytes());
            strings.extend(data);
        }
    }
    let subfamily = match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        _ => "Regular",
    };
    for (id, text) in [
        (1, family.to_string()),
        (2, subfamily.into()),
        (4, family.to_string()),
        (
            6,
            family.chars().filter(char::is_ascii_alphanumeric).collect(),
        ),
    ] {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for v in [
            3u16,
            1,
            0x409,
            id,
            encoded.len() as u16,
            strings.len() as u16,
        ] {
            records.extend(v.to_be_bytes());
        }
        strings.extend(encoded);
    }
    let count_names = records.len() / 12;
    if strings.len() > 60000 || count_names > 1000 {
        return None;
    }
    let mut name = vec![0, 0];
    name.extend((count_names as u16).to_be_bytes());
    name.extend((6 + records.len() as u16).to_be_bytes());
    name.extend(records);
    name.extend(strings);
    tables.insert(*b"name", name);
    // PDF subsets often omit OS/2, although desktop font loaders require it.
    // Restore metrics from the subset itself, without substituting glyphs.
    if !tables.contains_key(b"OS/2") {
        let hhea = tables.get(b"hhea")?;
        let head = tables.get(b"head")?;
        let mut os2 = vec![0u8; 96];
        let mut set = |offset: usize, value: u16| {
            os2[offset..offset + 2].copy_from_slice(&value.to_be_bytes())
        };
        set(0, 4);
        set(2, 500);
        set(4, if bold { 700 } else { 400 });
        set(6, 5);
        set(
            62,
            if bold {
                32
            } else if italic {
                1
            } else {
                64
            },
        );
        set(64, 0);
        set(66, 0xffff);
        let asc = u16::from_be_bytes(hhea.get(4..6)?.try_into().ok()?);
        let desc = i16::from_be_bytes(hhea.get(6..8)?.try_into().ok()?);
        set(68, asc);
        set(70, desc as u16);
        set(72, 0);
        set(74, u16::from_be_bytes(head.get(42..44)?.try_into().ok()?));
        set(
            76,
            i16::from_be_bytes(head.get(38..40)?.try_into().ok()?).unsigned_abs(),
        );
        os2[42..46].copy_from_slice(&15u32.to_be_bytes());
        os2[58..62].copy_from_slice(b"PDFN");
        os2[78..82].copy_from_slice(&1u32.to_be_bytes());
        tables.insert(*b"OS/2", os2);
    }
    let count = tables.len();
    let head = tables.get_mut(b"head")?;
    let style = (u16::from(bold)) | (u16::from(italic) << 1);
    let previous = u16::from_be_bytes(head.get(44..46)?.try_into().ok()?);
    head[44..46].copy_from_slice(&((previous & !3) | style).to_be_bytes());
    if let Some(os2) = tables.get_mut(b"OS/2")
        && os2.len() >= 64
    {
        let old = u16::from_be_bytes(os2[62..64].try_into().ok()?);
        let bits =
            (u16::from(italic)) | (u16::from(bold) << 5) | (u16::from(!bold && !italic) << 6);
        os2[62..64].copy_from_slice(&((old & !97) | bits).to_be_bytes());
    }
    let head = tables.get_mut(b"head")?;
    head.get_mut(8..12)?.fill(0);
    let mut out = bytes.get(..12)?.to_vec();
    if tables.contains_key(b"glyf") {
        out[..4].copy_from_slice(&[0, 1, 0, 0]);
    }
    out[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    let power = 1u16 << (count as u16).ilog2();
    out[6..8].copy_from_slice(&(power * 16).to_be_bytes());
    out[8..10].copy_from_slice(&(power.ilog2() as u16).to_be_bytes());
    out[10..12].copy_from_slice(&((count as u16 - power) * 16).to_be_bytes());
    out.resize(12 + count * 16, 0);
    let mut head_at = 0;
    for (i, (tag, data)) in tables.iter().enumerate() {
        let at = 12 + i * 16;
        let offset = out.len();
        out[at..at + 4].copy_from_slice(tag);
        out[at + 4..at + 8].copy_from_slice(&checksum(data).to_be_bytes());
        out[at + 8..at + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        out[at + 12..at + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_at = offset;
        }
        out.extend(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    out[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    Some(out)
}

pub(super) fn preserve(e: &Editor, page: usize, items: &mut [Item]) -> Result<Vec<SourceFont>> {
    let mut result = Vec::new();
    let sources =
        pdfcraft_edit::text::text_editor_fonts(&e.doc, page).map_err(|e| e.to_string())?;
    for source in sources {
        let family = super::layout::font(&source.face);
        let Ok(face) = ttf_parser::Face::parse(&source.bytes, 0) else {
            continue;
        };
        // Preserve already embedded PDF subsets; explicit restrictions still
        // forbid using a font in an editable Office document.
        if (face.tables().os2.is_some() && !face.is_outline_embedding_allowed())
            || matches!(
                face.permissions(),
                Some(
                    ttf_parser::Permissions::Restricted | ttf_parser::Permissions::PreviewAndPrint
                )
            )
        {
            continue;
        }
        let alias = format!("{family} PDF {} {}", page + 1, result.len() + 1);
        let bold = source.face.to_lowercase().contains("bold");
        let italic = source.face.to_lowercase().contains("italic");
        if installed_face_matches(&face, &family, bold, italic) {
            // Use the real installed name when every source glyph has the same
            // metrics and bounds. Writer does not consistently load DOCX aliases.
            continue;
        }
        let Some(bytes) = rename_styled(&source.bytes, &alias, bold, italic) else {
            continue;
        };
        let mut used = false;
        let mut visit = |p: &mut Paragraph| {
            for run in &mut p.runs {
                used |= assign(run, &face, &family, &alias, bold, italic);
            }
        };
        for item in items.iter_mut() {
            match item {
                Item::Paragraph(p) => visit(p),
                Item::Table(t) => {
                    for row in &mut t.rows {
                        for cell in row {
                            for p in &mut cell.paragraphs {
                                visit(p);
                            }
                        }
                    }
                }
                Item::Picture(_) => {}
            }
        }
        if used {
            result.push(SourceFont {
                name: alias,
                bytes,
                bold,
                italic,
            });
        }
    }
    Ok(result)
}

fn installed_face_matches(
    source: &ttf_parser::Face<'_>,
    family: &str,
    bold: bool,
    italic: bool,
) -> bool {
    let names = match family {
        "Arial" => ["arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf"],
        "Times New Roman" => ["times.ttf", "timesbd.ttf", "timesi.ttf", "timesbi.ttf"],
        "Courier New" => ["cour.ttf", "courbd.ttf", "couri.ttf", "courbi.ttf"],
        _ => return false,
    };
    let Some(windows) = std::env::var_os("WINDIR") else {
        return false;
    };
    let Ok(bytes) = std::fs::read(
        std::path::Path::new(&windows)
            .join("Fonts")
            .join(names[usize::from(bold) + 2 * usize::from(italic)]),
    ) else {
        return false;
    };
    let Ok(local) = ttf_parser::Face::parse(&bytes, 0) else {
        return false;
    };
    let Some(cmap) = source.tables().cmap else {
        return false;
    };
    let mut matched = 0;
    let mut same = true;
    let su = source.units_per_em() as f64;
    let lu = local.units_per_em() as f64;
    for table in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
        table.codepoints(|code| {
            let Some(ch) = char::from_u32(code).filter(|c| !c.is_control()) else {
                return;
            };
            let Some(a) = source.glyph_index(ch) else {
                return;
            };
            let Some(b) = local.glyph_index(ch) else {
                same = false;
                return;
            };
            matched += 1;
            let advance = source
                .glyph_hor_advance(a)
                .map(|n| n as f64 / su)
                .zip(local.glyph_hor_advance(b).map(|n| n as f64 / lu));
            same &= advance.is_some_and(|(a, b)| (a - b).abs() < 0.001);
            if let Some(a) = source.glyph_bounding_box(a) {
                if let Some(b) = local.glyph_bounding_box(b) {
                    same &= [a.x_min, a.y_min, a.x_max, a.y_max]
                        .into_iter()
                        .zip([b.x_min, b.y_min, b.x_max, b.y_max])
                        .all(|(a, b)| (a as f64 / su - b as f64 / lu).abs() < 0.002);
                } else {
                    same = false;
                }
            }
        });
    }
    same && matched >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn office_font_keeps_source_glyphs_metrics_rights_and_copyright() {
        let source = include_bytes!("../../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf");
        let bytes = rename(source, "Office source test").unwrap();
        assert_eq!(checksum(&bytes), 0xB1B0_AFBA);
        let a = ttf_parser::Face::parse(source, 0).unwrap();
        let b = ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert_eq!(a.permissions(), b.permissions());
        assert_eq!(a.units_per_em(), b.units_per_em());
        for ch in "Construction, école, 123".chars() {
            assert_eq!(a.glyph_index(ch), b.glyph_index(ch));
            let glyph = a.glyph_index(ch).unwrap();
            assert_eq!(a.glyph_hor_advance(glyph), b.glyph_hor_advance(glyph));
            assert_eq!(a.glyph_bounding_box(glyph), b.glyph_bounding_box(glyph));
        }
        for original in a
            .names()
            .into_iter()
            .filter(|n| matches!(n.name_id, 0 | 13 | 14))
        {
            assert!(
                b.names()
                    .into_iter()
                    .any(|n| n.name_id == original.name_id && n.name == original.name)
            );
        }
        assert!(
            b.names()
                .into_iter()
                .any(|n| n.name_id == 1 && n.to_string().as_deref() == Some("Office source test"))
        );
    }
    #[test]
    fn source_subset_without_os2_is_preserved_when_its_family_is_missing() {
        let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") else {
            return;
        };
        let e = Editor::open(std::path::Path::new(&path)).unwrap();
        let page = super::super::layout::page(&e, 0).unwrap();
        assert!(
            page.fonts
                .iter()
                .any(|f| f.name.starts_with("Century Schoolbook PDF")),
            "PDF subsets without OS/2 are already embedded document content, not fonts with an explicit bitmap-only restriction"
        );
    }
}
