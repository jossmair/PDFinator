use super::*;
use crate::drawing::Kind;

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn touches(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] <= b[2] + 1.5 && b[0] <= a[2] + 1.5 && a[1] <= b[3] + 1.5 && b[1] <= a[3] + 1.5
}
fn contains(r: [f64; 4], p: [f64; 2]) -> bool {
    p[0] >= r[0] - 1. && p[0] <= r[2] + 1. && p[1] >= r[1] - 1. && p[1] <= r[3] + 1.
}
fn center(r: [f64; 4]) -> [f64; 2] {
    [(r[0] + r[2]) / 2., (r[1] + r[3]) / 2.]
}
fn edges(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| (*a - *b).abs() < 1.5);
    values
}
fn view_rect(p: &pdfcraft_render::PageInfo, r: [f64; 4]) -> [f64; 4] {
    let a = p.user_to_view(r[0] as f32, r[1] as f32);
    let b = p.user_to_view(r[2] as f32, r[3] as f32);
    [
        a[0].min(b[0]) as f64,
        a[1].min(b[1]) as f64,
        a[0].max(b[0]) as f64,
        a[1].max(b[1]) as f64,
    ]
}
fn white_paper(e: &Editor, index: usize, target: crate::selection::Target) -> bool {
    let Ok(bytes) = e.selection_preview(index, &[target], true, false) else {
        return false;
    };
    let page = &e.info.pages[index];
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    let raster = renderer.render(pdfcraft_render::RenderRequest {
        page: index,
        scale: (1024. / page.width.max(page.height)).min(1.),
        ..Default::default()
    });
    raster.error.is_none()
        && raster
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 0 || pixel[..3].iter().all(|c| *c >= 254))
}
// Some producers give every tiny Form a page-sized BBox. Measure its actual
// ink in isolation instead of dropping its geometry or flattening the page.
fn form_ink_rect(e: &Editor, page: usize, image: usize) -> Option<[f64; 4]> {
    let target = crate::selection::Target::Object(pdfcraft_edit::ObjectTarget {
        kind: pdfcraft_edit::ObjectKind::Image,
        index: image,
    });
    let bytes = e.selection_preview(page, &[target], true, false).ok()?;
    let info = &e.info.pages[page];
    let scale = (1536. / info.width.max(info.height)).min(2.);
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    let raster = renderer.render(pdfcraft_render::RenderRequest {
        page,
        scale,
        ..Default::default()
    });
    if raster.error.is_some() {
        return None;
    }
    let mut rect = [raster.width, raster.height, 0, 0];
    for (n, px) in raster.rgba.as_chunks::<4>().0.iter().enumerate() {
        if px[3] > 0 && px[..3].iter().any(|v| *v < 245) {
            let x = n as u32 % raster.width;
            let y = n as u32 / raster.width;
            rect = [
                rect[0].min(x),
                rect[1].min(y),
                rect[2].max(x + 1),
                rect[3].max(y + 1),
            ];
        }
    }
    (rect[2] > rect[0] && rect[3] > rect[1]).then(|| {
        [
            rect[0] as f64 * info.width as f64 / raster.width as f64,
            rect[1] as f64 * info.height as f64 / raster.height as f64,
            rect[2] as f64 * info.width as f64 / raster.width as f64,
            rect[3] as f64 * info.height as f64 / raster.height as f64,
        ]
    })
}
fn visible_header(
    e: &Editor,
    index: usize,
    p: &pdfcraft_render::PageInfo,
    t: &Table,
    top: f64,
) -> bool {
    if top < 0. || t.rect[1] - top < 4. {
        return false;
    }
    let scale = (2048. / p.width.max(p.height)).min(1.5);
    let mut renderer = pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page: index,
        scale,
        ..Default::default()
    });
    if image.error.is_some() {
        return false;
    }
    let ink = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
            return false;
        }
        let at = (y as usize * image.width as usize + x as usize) * 4;
        image
            .rgba
            .get(at..at + 3)
            .is_some_and(|v| v.iter().copied().min().unwrap_or(255) < 235)
    };
    let s = scale as f64;
    let (x0, x1, y0, y1) = (
        (t.rect[0] * s).round() as i32,
        (t.rect[2] * s).round() as i32,
        (top * s).round() as i32,
        (t.rect[1] * s).round() as i32,
    );
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let horizontal = (-2..=2)
        .any(|d| (x0..x1).filter(|x| ink(*x, y0 + d)).count() as f64 / (x1 - x0) as f64 > 0.75);
    horizontal
        && t.xs.iter().all(|x| {
            let x = (x * s).round() as i32;
            (-2..=2).any(|d| {
                (y0..y1).filter(|y| ink(x + d, *y)).count() as f64 / (y1 - y0) as f64 > 0.7
            })
        })
}
pub(super) fn font(name: &str) -> String {
    let name = name.split_once('+').map_or(name, |(_, n)| n);
    let name = name
        .strip_suffix("-Identity-H")
        .or_else(|| name.strip_suffix("-Identity-V"))
        .unwrap_or(name);
    let lower = name.to_lowercase();
    if lower.contains("times") {
        "Times New Roman".into()
    } else if lower.contains("helvetica") || lower.starts_with("arial") {
        "Arial".into()
    } else if lower.contains("courier") {
        "Courier New".into()
    } else if lower.contains("centuryschoolbook") {
        "Century Schoolbook".into()
    } else if lower.contains("liberationserif") {
        "Liberation Serif".into()
    } else if lower.contains("liberationsans") {
        "Liberation Sans".into()
    } else {
        name.replace([',', '-'], " ")
            .replace("BoldItalic", "")
            .replace("Bold", "")
            .replace("Italic", "")
            .replace("Regular", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}
fn paragraphs(e: &Editor, index: usize) -> Result<Vec<Paragraph>> {
    let page = &e.info.pages[index];
    let reading =
        pdfcraft_edit::reading_blocks_checked(&e.doc, index).map_err(|e| e.to_string())?;
    if reading.truncated {
        return Err(format!(
            "La page {} est trop complexe pour être exportée intégralement.",
            index + 1
        ));
    }
    let metadata: Vec<_> = e
        .blocks(index)?
        .iter()
        .enumerate()
        .filter_map(|(i, _)| e.text_formatting(index, i))
        .collect();
    let native_blocks = e.blocks(index)?;
    let native_lines = pdfcraft_edit::text_lines(&e.doc, index).map_err(|e| e.to_string())?;
    let unit = f64::from(if page.rotation.is_multiple_of(180) {
        page.width
    } else {
        page.height
    }) / f64::from(page.crop[2] - page.crop[0]).max(0.001);
    Ok(reading
        .blocks
        .into_iter()
        .filter(|b| !b.text.trim().is_empty() && !b.invisible)
        .map(|b| {
            let native = native_blocks.iter().find(|n| {
                n.editor_text == b.editor_text
                    && (n.rect[0] - b.rect[0]).abs() < 2.
                    && (n.rect[3] - b.rect[3]).abs() < 2.
            });
            let data = metadata.iter().find(|d| {
                (d.rect[0] - b.rect[0]).abs() < 2.
                    && (d.rect[3] - b.rect[3]).abs() < b.size * 2.
                    && d.text.replace('\n', " ") == b.editor_text.replace('\n', " ")
            });
            let text = data.map_or(b.editor_text.as_str(), |d| &d.text);
            let mut boundaries = vec![0, text.len()];
            if data.is_none()
                && let Some(n) = native
            {
                for (range, _) in &n.spans {
                    boundaries.extend([range.start, range.end]);
                }
            }
            if let Some(d) = data {
                for span in &d.spans {
                    boundaries.extend([span.range.start, span.range.end]);
                }
            }
            boundaries.sort_unstable();
            boundaries.dedup();
            let runs = boundaries
                .windows(2)
                .filter_map(|r| {
                    let text_run = text.get(r[0]..r[1])?;
                    let source = native
                        .and_then(|n| n.spans.iter().find(|(range, _)| range.contains(&r[0])))
                        .and_then(|(_, line)| native_lines.get(*line));
                    let f = data
                        .and_then(|d| d.spans.iter().find(|s| s.range.contains(&r[0])))
                        .map(|s| s.format.clone())
                        .unwrap_or_default();
                    Some(Run {
                        text: text_run.into(),
                        font: font(
                            f.font_name
                                .as_deref()
                                .unwrap_or(source.map_or(&b.base_font, |l| &l.base_font)),
                        ),
                        size: f.size.unwrap_or(source.map_or(b.size, |l| l.size)) * unit,
                        color: f.color.unwrap_or(source.map_or(b.color, |l| l.color)),
                        bold: f.bold.unwrap_or(source.map_or(b.bold, |l| l.bold)),
                        italic: f.italic.unwrap_or(source.map_or(b.italic, |l| l.italic)),
                        underline: f.underline.unwrap_or(false),
                        strike: f.strike.unwrap_or(false),
                        script: f.script.unwrap_or(0),
                        baseline: source.map(|l| {
                            let b = l.baseline();
                            page.user_to_view(b[0] as f32, b[1] as f32)[1] as f64
                        }),
                        rise: None,
                        background: f
                            .highlight
                            .unwrap_or(f.background.is_some())
                            .then(|| f.background.unwrap_or([1., 0.9, 0.25])),
                        rect: source.map(|l| view_rect(page, l.rect)),
                        fraction: None,
                    })
                })
                .collect();
            Paragraph {
                rect: view_rect(page, b.rect),
                runs,
                align: data
                    .and_then(|d| d.align.as_deref())
                    .map_or("left", |a| match a {
                        "Centre" => "center",
                        "Droite" => "right",
                        "Justifié" => "both",
                        _ => "left",
                    }),
                spacing: data.and_then(|d| d.line_spacing),
            }
        })
        .collect())
}
#[derive(Clone)]
struct Line {
    a: [f64; 2],
    b: [f64; 2],
    width: f64,
    color: [f64; 3],
}
impl Line {
    fn rect(&self) -> [f64; 4] {
        [
            self.a[0].min(self.b[0]),
            self.a[1].min(self.b[1]),
            self.a[0].max(self.b[0]),
            self.a[1].max(self.b[1]),
        ]
    }
    fn vertical(&self) -> bool {
        (self.a[0] - self.b[0]).abs() < 1.
    }
}
fn groups(rects: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let mut used = vec![false; rects.len()];
    let mut groups = Vec::new();
    for i in 0..rects.len() {
        if used[i] {
            continue;
        }
        let mut members = vec![i];
        used[i] = true;
        let mut n = 0;
        while n < members.len() {
            for j in 0..rects.len() {
                if !used[j] && touches(rects[members[n]], rects[j]) {
                    used[j] = true;
                    members.push(j);
                }
            }
            n += 1;
        }
        groups.push(members);
    }
    groups
}
fn figure_groups(rects: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let padded: Vec<_> = rects
        .iter()
        .map(|r| [r[0] - 8., r[1] - 8., r[2] + 8., r[3] + 8.])
        .collect();
    groups(&padded)
}
fn complex_math(p: &Paragraph, dense: bool) -> bool {
    let fractions: Vec<_> = p
        .runs
        .iter()
        .filter_map(|r| r.fraction.as_ref().map(|f| (r, f)))
        .collect();
    let scripts = p
        .runs
        .iter()
        .filter(|r| r.rise.is_some() || r.script != 0)
        .count()
        + p.runs
            .iter()
            .flat_map(|r| r.text.chars())
            .filter(|c| matches!(c, '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹'))
            .count();
    fractions.len() >= 2
        || fractions.iter().any(|(r, f)| {
            dense
                || r.size < 10.
                || [&f.numerator, &f.denominator].into_iter().any(|rs| {
                    rs.iter()
                        .flat_map(|r| r.text.chars())
                        .filter(|c| !c.is_whitespace())
                        .count()
                        >= 3
                })
        })
        || (scripts >= 2
            && p.runs
                .iter()
                .flat_map(|r| r.text.chars())
                .any(|c| "+=×÷√".contains(c)))
        || p.runs.iter().any(|r| {
            r.text.chars().any(|c| "()[]{}".contains(c))
                && r.rect
                    .is_some_and(|b| b[3] > b[1] && r.size > (b[3] - b[1]) * 1.8)
        })
}
fn preserve_complex_math(e: &Editor, page: usize, items: &mut Vec<Item>) -> Result<()> {
    fn run_bounds(runs: &[Run], rect: &mut [f64; 4]) {
        for r in runs {
            if let Some(b) = r.rect {
                *rect = union(*rect, b);
            }
            if let Some(f) = &r.fraction {
                run_bounds(&f.numerator, rect);
                run_bounds(&f.denominator, rect);
            }
        }
    }
    let dense = items
        .iter()
        .filter(|i| matches!(i,Item::Paragraph(p) if p.runs.iter().any(|r| r.fraction.is_some())))
        .count()
        >= 6;
    let candidates: Vec<_> = items
        .iter()
        .filter_map(|i| {
            if let Item::Paragraph(p) = i {
                complex_math(p, dense).then(|| {
                    let mut r = p.rect;
                    run_bounds(&p.runs, &mut r);
                    r
                })
            } else {
                None
            }
        })
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }
    if candidates.len() > 512 {
        return Err("La page contient trop de blocs mathématiques complexes.".into());
    }
    let info = &e.info.pages[page];
    let mut renderer = pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
    let raster = renderer.render(pdfcraft_render::RenderRequest {
        page,
        scale: (4096. / info.width.max(info.height)).min(3.),
        ..Default::default()
    });
    if let Some(error) = raster.error {
        return Err(error);
    }
    let sx = raster.width as f64 / info.width as f64;
    let sy = raster.height as f64 / info.height as f64;
    let rects: Vec<_> = groups(&candidates)
        .into_iter()
        .filter_map(|g| g.into_iter().map(|i| candidates[i]).reduce(union))
        .map(|mut r| {
            // Numerators can sit above the paragraph baseline. Preserve the
            // complete intersecting text block instead of erasing its top or
            // leaving a second, shifted native fragment over the source ink.
            for _ in 0..items.len().min(512) {
                let before = r;
                for item in items.iter() {
                    if let Item::Paragraph(p) = item {
                        let b = p.rect;
                        if r[0] < b[2] && b[0] < r[2] && r[1] < b[3] && b[1] < r[3] {
                            r = union(r, b);
                        }
                    }
                }
                if before == r {
                    break;
                }
            }
            [
                (r[0] - 1.).max(0.),
                (r[1] - 1.).max(0.),
                (r[2] + 1.).min(info.width as f64),
                (r[3] + 1.).min(info.height as f64),
            ]
        })
        .collect();
    let mut pictures = Vec::new();
    let mut preserved_pixels = 0u64;
    for r in &rects {
        let x0 = (r[0] * sx).floor() as u32;
        let x1 = (r[2] * sx).ceil().min(raster.width as f64) as u32;
        let y0 = (r[1] * sy).floor() as u32;
        let y1 = (r[3] * sy).ceil().min(raster.height as f64) as u32;
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        preserved_pixels += u64::from(x1 - x0) * u64::from(y1 - y0);
        if preserved_pixels > 32 * 1024 * 1024 {
            return Err(
                "Les blocs mathématiques de cette page dépassent le budget de conversion.".into(),
            );
        }
        let mut rgba = Vec::new();
        for y in y0..y1 {
            rgba.extend_from_slice(
                &raster.rgba[((y * raster.width + x0) * 4) as usize
                    ..((y * raster.width + x1) * 4) as usize],
            );
        }
        pictures.push(Item::Picture(Picture {
            rect: *r,
            bytes: crate::ocr::encode_png(&rgba, x1 - x0, y1 - y0)?,
        }));
    }
    items.retain(
        |i| !matches!(i,Item::Paragraph(p) if rects.iter().any(|r| contains(*r,center(p.rect)))),
    );
    items.extend(pictures);
    Ok(())
}
fn split_rows(p: &Paragraph) -> Vec<Paragraph> {
    let count = p
        .runs
        .iter()
        .map(|r| r.text.matches('\n').count())
        .sum::<usize>()
        + 1;
    let height = (p.rect[3] - p.rect[1]) / count as f64;
    let mut rows = vec![Paragraph {
        rect: p.rect,
        runs: Vec::new(),
        align: p.align,
        spacing: p.spacing,
    }];
    for r in &p.runs {
        for (i, part) in r.text.split('\n').enumerate() {
            if i > 0 {
                rows.push(Paragraph {
                    rect: p.rect,
                    runs: Vec::new(),
                    align: p.align,
                    spacing: p.spacing,
                });
            }
            if !part.is_empty() {
                let mut run = r.clone();
                run.text = part.into();
                rows.last_mut().unwrap().runs.push(run);
            }
        }
    }
    for (i, row) in rows.iter_mut().enumerate() {
        row.rect[1] = p.rect[1] + i as f64 * height;
        row.rect[3] = row.rect[1] + height;
    }
    rows
}

/// PDF runs commonly change font at a word or even a superscript. Reassemble
/// one visual line before inferring columns, so styles do not create columns.
fn join_lines(mut input: Vec<Paragraph>, gap: f64) -> Vec<Paragraph> {
    input.sort_by(|a, b| {
        a.rect[1]
            .total_cmp(&b.rect[1])
            .then(a.rect[0].total_cmp(&b.rect[0]))
    });
    let mut rows: Vec<Vec<Paragraph>> = Vec::new();
    for p in input {
        let size = p.runs.iter().map(|r| r.size).fold(1., f64::max);
        let row = rows.iter_mut().rev().take(12).find(|row| {
            let bounds = row.iter().map(|p| p.rect).reduce(union).unwrap();
            let overlap = bounds[3].min(p.rect[3]) - bounds[1].max(p.rect[1]);
            let row_size = row
                .iter()
                .flat_map(|p| &p.runs)
                .map(|r| r.size)
                .fold(1., f64::max);
            let small = if size < row_size { &p } else { &row[0] };
            let script = size.min(row_size) <= size.max(row_size) * 0.8
                && size.min(row_size) >= size.max(row_size) * 0.4
                && small
                    .runs
                    .iter()
                    .map(|r| r.text.chars().count())
                    .sum::<usize>()
                    <= 3
                && ((bounds[1] + bounds[3] - p.rect[1] - p.rect[3]) / 2.).abs()
                    < size.max(row_size) * 1.2
                && (p.rect[0] - bounds[2]).max(bounds[0] - p.rect[2]) < size.min(row_size) * 0.75;
            (overlap > (bounds[3] - bounds[1]).min(p.rect[3] - p.rect[1]) * 0.35 || script)
                && (p.rect[0] - bounds[2]).max(bounds[0] - p.rect[2]) < gap * size
        });
        if let Some(row) = row {
            row.push(p);
        } else {
            rows.push(vec![p]);
        }
    }
    // A raised glyph can arrive before the words of its base line. Once those
    // words have assembled, reattach the orphan rather than treating it as a
    // separate paragraph/column and consuming another line of page height.
    for i in 0..rows.len() {
        let count = rows[i]
            .iter()
            .flat_map(|p| &p.runs)
            .map(|r| r.text.chars().count())
            .sum::<usize>();
        if count == 0 || count > 3 {
            continue;
        }
        let Some(bounds) = rows[i].iter().map(|p| p.rect).reduce(union) else {
            continue;
        };
        let size = rows[i]
            .iter()
            .flat_map(|p| &p.runs)
            .map(|r| r.size)
            .fold(1., f64::max);
        let candidate = (i.saturating_sub(12)..(i + 13).min(rows.len()))
            .filter(|j| *j != i)
            .filter_map(|j| {
                let other = rows[j].iter().map(|p| p.rect).reduce(union)?;
                let max = rows[j]
                    .iter()
                    .flat_map(|p| &p.runs)
                    .map(|r| r.size)
                    .fold(1., f64::max);
                let dy = ((other[1] + other[3] - bounds[1] - bounds[3]) / 2.).abs();
                (size <= max * 0.8
                    && size >= max * 0.4
                    && dy < max * 1.2
                    && (bounds[0] - other[2]).max(other[0] - bounds[2]) < size * 0.75)
                    .then_some((j, dy))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0);
        if let Some(j) = candidate {
            let extra = std::mem::take(&mut rows[i]);
            rows[j].extend(extra);
        }
    }
    rows.retain(|row| !row.is_empty());
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.rect[0].total_cmp(&b.rect[0]));
            let max_size = row
                .iter()
                .flat_map(|p| p.runs.iter())
                .filter(|r| r.fraction.is_none())
                .map(|r| r.size)
                .fold(1_f64, f64::max);
            let mut baselines: Vec<_> = row
                .iter()
                .flat_map(|p| &p.runs)
                .filter(|r| r.fraction.is_none() && r.script == 0 && r.size >= max_size * 0.8)
                .filter_map(|r| r.baseline)
                .collect();
            baselines.sort_by(f64::total_cmp);
            if let Some(&baseline) = baselines.get(baselines.len() / 2) {
                for r in row.iter_mut().flat_map(|p| &mut p.runs) {
                    if r.fraction.is_none()
                        && r.script == 0
                        && r.size < max_size * 0.8
                        && let Some(source) = r.baseline
                    {
                        let rise = baseline - source;
                        if rise.abs() > max_size * 0.15 {
                            r.script = if rise > 0. { 1 } else { -1 };
                            r.rise = Some(rise);
                        }
                    }
                }
            }
            let mut result = row.remove(0);
            for p in row {
                let size = result.runs.last().map_or(11., |r| r.size);
                let spaces = ((p.rect[0] - result.rect[2]) / (size * 0.3))
                    .round()
                    .clamp(0., 20.) as usize;
                if spaces > 0
                    && let Some(last) = result.runs.last_mut()
                    && !last.text.ends_with(char::is_whitespace)
                {
                    last.text.push_str(&" ".repeat(spaces));
                }
                result.rect = union(result.rect, p.rect);
                result.runs.extend(p.runs);
            }
            if result.runs.iter().any(|r| r.rise.is_some())
                && result.runs.iter().all(|r| {
                    r.rect.is_some() && !r.text.chars().any(|c| matches!(c as u32, 0x0590..=0x08ff))
                })
            {
                result
                    .runs
                    .sort_by(|a, b| a.rect.unwrap()[0].total_cmp(&b.rect.unwrap()[0]));
            }
            if (0..result.runs.len()).any(|i| angle(&result.runs, i).is_some())
                && let Some(bounds) = result
                    .runs
                    .iter()
                    .find(|r| r.text.trim() != "^" && r.script == 0)
                    .and_then(|r| r.rect)
            {
                result.rect[1] = bounds[1];
            }
            result
        })
        .collect()
}
fn grid(lines: &[Line], paragraphs: &[Paragraph], fills: &[([f64; 4], [f64; 3])]) -> Option<Table> {
    let xs = edges(
        lines
            .iter()
            .filter(|l| l.vertical())
            .map(|l| l.a[0])
            .collect(),
    );
    let ys = edges(
        lines
            .iter()
            .filter(|l| !l.vertical())
            .map(|l| l.a[1])
            .collect(),
    );
    if !(2..=64).contains(&xs.len()) || !(2..=201).contains(&ys.len()) {
        return None;
    }
    let rect = [xs[0], ys[0], *xs.last()?, *ys.last()?];
    let text: Vec<_> = paragraphs
        .iter()
        .flat_map(|p| {
            p.runs.iter().map(|r| Paragraph {
                rect: r.rect.unwrap_or(p.rect),
                runs: vec![r.clone()],
                align: p.align,
                spacing: p.spacing,
            })
        })
        .filter(|p| contains(rect, center(p.rect)))
        .collect();
    // A coordinate grid with only a few labels is artwork, not a data table.
    let empty_form = ys.len() == 2 && xs.len() >= 3;
    if !empty_form
        && (ys.len() < 3 || text.len() < 3 || text.len() * 5 < (xs.len() - 1) * (ys.len() - 1))
    {
        return None;
    }
    let boundary = |vertical: bool, at: f64, from: f64, to: f64| {
        lines.iter().any(|l| {
            let r = l.rect();
            if vertical {
                l.vertical() && (r[0] - at).abs() < 1.5 && r[1] <= from + 1.5 && r[3] >= to - 1.5
            } else {
                !l.vertical() && (r[1] - at).abs() < 1.5 && r[0] <= from + 1.5 && r[2] >= to - 1.5
            }
        })
    };
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    for y in 0..ys.len() - 1 {
        let mut row = Vec::new();
        let mut x = 0;
        while x < xs.len() - 1 {
            let start = x;
            x += 1;
            while x < xs.len() - 1 && !boundary(true, xs[x], ys[y], ys[y + 1]) {
                x += 1;
            }
            let cell_rect = [xs[start], ys[y], xs[x], ys[y + 1]];
            let continues = y > 0
                && !boundary(false, ys[y], xs[start], xs[x])
                && rows[y - 1]
                    .iter()
                    .any(|c| c.column == start && c.span == x - start);
            if continues && let Some(previous) = rows[y - 1].iter_mut().find(|c| c.column == start)
            {
                previous.merge.get_or_insert("restart");
            }
            let fill = fills
                .iter()
                .rev()
                .find(|(r, _)| contains(*r, center(cell_rect)))
                .map(|(_, c)| *c);
            row.push(Cell {
                column: start,
                span: x - start,
                merge: continues.then_some("continue"),
                fill,
                paragraphs: join_lines(
                    text.iter()
                        .filter(|p| contains(cell_rect, center(p.rect)))
                        .cloned()
                        .collect(),
                    f64::INFINITY,
                )
                .into_iter()
                .map(|mut p| {
                    let center_gap =
                        ((p.rect[0] + p.rect[2]) / 2. - (cell_rect[0] + cell_rect[2]) / 2.).abs();
                    if (p.rect[0] - cell_rect[0] > 8. && center_gap < 6.)
                        || (y == 0 && cell_rect[2] - p.rect[2] < 4.5 && center_gap < 6.)
                    {
                        p.align = "center";
                    }
                    p
                })
                .collect(),
            });
        }
        rows.push(row);
    }
    let border = lines
        .iter()
        .map(|l| l.width)
        .fold(f64::INFINITY, f64::min)
        .clamp(0.125, 12.);
    Some(Table {
        rect,
        xs,
        ys,
        rows,
        border,
        color: lines[0].color,
    })
}

/// Match both sides of a visible fraction bar before reading-order joining.
/// Its numerator and denominator can belong to different PDF text objects.
fn fractions(paragraphs: Vec<Paragraph>, lines: &[Line]) -> Vec<Paragraph> {
    let runs: Vec<_> = paragraphs
        .iter()
        .flat_map(|p| p.runs.iter().cloned())
        .collect();
    let mut used = vec![false; runs.len()];
    let mut found = Vec::new();
    for line in lines.iter().filter(|l| (l.a[1] - l.b[1]).abs() < 1.) {
        let bar = line.rect();
        if !(3. ..=100.).contains(&(bar[2] - bar[0])) {
            continue;
        }
        let side = |above: bool| -> Vec<usize> {
            let mut prose = false;
            let indices = runs
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let b = r.rect?;
                    let value = r.text.trim();
                    let short = value.chars().count() <= 3;
                    let math = value.chars().all(|c| {
                        c.is_ascii_digit()
                            || (short && c.is_alphabetic())
                            || c.is_whitespace()
                            || matches!(
                                c,
                                '+' | '-'
                                    | '−'
                                    | '±'
                                    | '∓'
                                    | '·'
                                    | '⋅'
                                    | '×'
                                    | '÷'
                                    | '='
                                    | '.'
                                    | ','
                                    | '('
                                    | ')'
                                    | '…'
                                    | '\u{e08b}'
                            )
                    });
                    let middle = (b[1] + b[3]) / 2.;
                    let ellipsis = r
                        .text
                        .trim()
                        .chars()
                        .all(|c| matches!(c, '\u{e08b}' | '…' | '.'));
                    let near = if above {
                        (middle <= bar[1] - r.size * 0.1
                            || (ellipsis && middle <= bar[1] - r.size * 0.25))
                            && b[3] >= bar[1] - r.size * 0.8
                    } else {
                        middle >= bar[1] + r.size * 0.1 && b[1] <= bar[1] + r.size * 0.8
                    };
                    let eligible = !used[i]
                        && near
                        && b[0] >= bar[0] - 2.
                        && b[2] <= bar[2] + 2.
                        && !r.text.trim().is_empty();
                    if eligible && !math {
                        prose = true;
                    }
                    (eligible && math).then_some(i)
                })
                .collect();
            if prose { Vec::new() } else { indices }
        };
        let numerator = side(true);
        let denominator = side(false);
        if numerator.is_empty() || denominator.is_empty() {
            continue;
        }
        let mut collect = |indices: &[usize]| {
            let mut out: Vec<_> = indices
                .iter()
                .map(|&i| {
                    used[i] = true;
                    let mut r = runs[i].clone();
                    r.underline = false;
                    if r.font == "OpenSymbol" && r.text.contains('\u{e08b}') {
                        r.text = r.text.replace('\u{e08b}', "…");
                        r.font = "Times New Roman".into();
                    }
                    r
                })
                .collect();
            out.sort_by(|a, b| a.rect.unwrap()[0].total_cmp(&b.rect.unwrap()[0]));
            out
        };
        let numerator = collect(&numerator);
        let denominator = collect(&denominator);
        let bounds = numerator
            .iter()
            .chain(&denominator)
            .filter_map(|r| r.rect)
            .reduce(union)
            .unwrap();
        let bounds = union(bounds, bar);
        let mut r = numerator[0].clone();
        r.rect = Some(bounds);
        r.text = format!(
            "{}/{}",
            numerator.iter().map(|r| r.text.trim()).collect::<String>(),
            denominator
                .iter()
                .map(|r| r.text.trim())
                .collect::<String>()
        );
        r.fraction = Some(Box::new(Fraction {
            numerator,
            denominator,
        }));
        found.push(Paragraph {
            rect: bounds,
            runs: vec![r],
            align: "left",
            spacing: None,
        });
    }
    if found.is_empty() {
        return paragraphs;
    }
    let mut offset = 0;
    let mut output = Vec::new();
    for mut p in paragraphs {
        let end = offset + p.runs.len();
        if used[offset..end].contains(&true) {
            p.runs = p
                .runs
                .into_iter()
                .enumerate()
                .filter_map(|(n, r)| (!used[offset + n]).then_some(r))
                .collect();
            if let Some(bounds) = p.runs.iter().filter_map(|r| r.rect).reduce(union) {
                p.rect = bounds;
            }
        }
        offset = end;
        if !p.runs.is_empty() {
            output.push(p);
        }
    }
    // A row of fractions is one editable paragraph with tab stops, rather than
    // independent columns with their own sequential pagination.
    let bounds: Vec<_> = found.iter().map(|p| p.rect).collect();
    let mut math = found;
    output.retain(|p| {
        let belongs = bounds
            .iter()
            .any(|b| p.rect[1] > b[1] + 2. && p.rect[3] < b[3] - 2.);
        if belongs {
            math.push(p.clone());
        }
        !belongs
    });
    output.extend(join_lines(math, f64::INFINITY));
    output
}

pub(super) fn page(e: &Editor, index: usize) -> Result<Page> {
    let p = &e.info.pages[index];
    let source = paragraphs(e, index)?;
    let mut paragraphs: Vec<_> = source.iter().flat_map(split_rows).collect();
    let native_lines = pdfcraft_edit::text_lines(&e.doc, index).map_err(|e| e.to_string())?;
    let text = e.text(index)?;
    let chars: Vec<_> = text
        .glyphs
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            g.text
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(move |c| (c, i))
        })
        .collect();
    for p in &mut paragraphs {
        let target: Vec<_> = p
            .runs
            .iter()
            .flat_map(|r| r.text.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        if target.is_empty() || target.len() > chars.len() {
            continue;
        }
        let mut best = None;
        for i in 0..=chars.len() - target.len() {
            if chars[i].0 != target[0] || !target.iter().zip(&chars[i..]).all(|(c, (a, _))| c == a)
            {
                continue;
            }
            let bounds = chars[i..i + target.len()]
                .iter()
                .map(|(_, n)| text.glyphs[*n].rect.map(f64::from))
                .reduce(union)
                .unwrap();
            let score = (bounds[1] - p.rect[1]).abs() * 4. + (bounds[0] - p.rect[0]).abs() * 0.2;
            if score < 80. && best.is_none_or(|(s, _, _)| score < s) {
                best = Some((score, bounds, i));
            }
        }
        if let Some((_, bounds, first)) = best {
            p.rect = bounds;
            let mut offset = first;
            let mut refined = Vec::new();
            for run in &mut p.runs {
                let count = run.text.chars().filter(|c| !c.is_whitespace()).count();
                if count > 0 {
                    run.rect = chars[offset..offset + count]
                        .iter()
                        .map(|(_, n)| text.glyphs[*n].rect.map(f64::from))
                        .reduce(union);
                }
                let mut at = offset;
                let mut begin = 0;
                for (i, c) in run.text.char_indices() {
                    if c.is_whitespace() {
                        if i > begin {
                            let mut part = run.clone();
                            part.text = run.text[begin..i].into();
                            let n = part.text.chars().count();
                            part.rect = chars[at..at + n]
                                .iter()
                                .map(|(_, g)| text.glyphs[*g].rect.map(f64::from))
                                .reduce(union);
                            at += n;
                            refined.push(part);
                        }
                        if let Some(last) = refined.last_mut() {
                            last.text.push(c);
                        }
                        begin = i + c.len_utf8();
                    }
                }
                if begin < run.text.len() {
                    let mut part = run.clone();
                    part.text = run.text[begin..].into();
                    let n = part.text.chars().count();
                    part.rect = chars[at..at + n]
                        .iter()
                        .map(|(_, g)| text.glyphs[*g].rect.map(f64::from))
                        .reduce(union);
                    refined.push(part);
                }
                offset += count;
            }
            p.runs = refined;
            // Reading-order blocks may combine a bold heading with regular
            // body rows. Recover each word's face from its actual PDF line.
            for r in &mut p.runs {
                if let Some(bounds) = r.rect
                    && let Some(line) = native_lines
                        .iter()
                        .filter(|l| {
                            contains(view_rect(&e.info.pages[index], l.rect), center(bounds))
                                && l.text.contains(r.text.trim())
                        })
                        .min_by(|a, b| {
                            let area = |l: &pdfcraft_edit::text::TextLine| {
                                (l.rect[2] - l.rect[0]) * (l.rect[3] - l.rect[1])
                            };
                            area(a).total_cmp(&area(b))
                        })
                {
                    r.font = font(&line.base_font);
                    r.bold = line.bold;
                    r.italic = line.italic;
                    r.color = line.color;
                    let baseline = line.baseline();
                    r.baseline = Some(
                        e.info.pages[index].user_to_view(baseline[0] as f32, baseline[1] as f32)[1]
                            as f64,
                    );
                }
            }
            if let Some(r) = p.runs.first_mut() {
                r.text = r.text.trim_start().into();
            }
            if let Some(r) = p.runs.last_mut() {
                r.text = r.text.trim_end().into();
            }
        }
    }
    let mut lines = Vec::new();
    let mut artwork = Vec::new();
    let mut fills = Vec::new();
    let mut decoration_targets = Vec::new();
    let mut decoration_regions = Vec::new();
    let drawings = e.export_drawings(index)?;
    if drawings.len() > 4000 {
        return Err(format!(
            "La page {} contient trop de tracés pour une conversion structurée.",
            index + 1
        ));
    }
    for d in drawings {
        let point = |x, y| {
            let a = d.at(x, y);
            p.user_to_view(a[0] as f32, a[1] as f32).map(f64::from)
        };
        let a = point(0., 0.);
        let b = point(1., 1.);
        let r = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ];
        match d.kind {
            Kind::Line => {
                if (a[0] - b[0]).abs() < 1. || (a[1] - b[1]).abs() < 1. {
                    lines.push(Line {
                        a,
                        b,
                        width: d.width,
                        color: d.color,
                    });
                } else {
                    artwork.push(r);
                }
            }
            Kind::Rectangle | Kind::Table => {
                for x in 0..=d.columns {
                    lines.push(Line {
                        width: d.width,
                        color: d.color,
                        a: point(x as f64 / d.columns as f64, 0.),
                        b: point(x as f64 / d.columns as f64, 1.),
                    });
                }
                for y in 0..=d.rows {
                    lines.push(Line {
                        width: d.width,
                        color: d.color,
                        a: point(0., y as f64 / d.rows as f64),
                        b: point(1., y as f64 / d.rows as f64),
                    });
                }
            }
            Kind::FilledRectangle => {
                fills.push((r, d.color));
            }
            Kind::Circle => {
                artwork.push(r);
            }
        }
    }
    // A source underline is a small vector stroke, not a missing character.
    for paragraph in &mut paragraphs {
        for run in &mut paragraph.runs {
            if run.fraction.is_some() {
                continue;
            }
            let bounds = run.rect.unwrap_or(paragraph.rect);
            let native_border = lines.iter().any(|l| {
                let r = l.rect();
                !l.vertical()
                    && (r[1] - bounds[3]).abs() < 2.5
                    && r[0] <= bounds[0] + 2.
                    && r[2] >= bounds[2] - 2.
                    && r[2] - r[0] < paragraph.rect[2] - paragraph.rect[0] + 12.
            });
            if native_border {
                run.underline = true;
            }
        }
    }
    let mut items = Vec::new();
    for group in groups(&lines.iter().map(Line::rect).collect::<Vec<_>>()) {
        let mut members: Vec<_> = group.iter().map(|&i| lines[i].clone()).collect();
        let mut detected = grid(&members, &paragraphs, &fills);
        if let Some(t) = &detected
            && t.ys.len() == 2
            && t.xs.len() >= 4
        {
            let top = t.ys[0] - (t.ys[1] - t.ys[0]);
            let native_border = lines.iter().any(|l| {
                !l.vertical()
                    && (l.a[1] - top).abs() < 2.
                    && l.rect()[0] <= t.rect[0] + 2.
                    && l.rect()[2] >= t.rect[2] - 2.
            });
            if native_border || visible_header(e, index, p, t, top) {
                let frame = [t.rect[0] - 2., top - 2., t.rect[2] + 2., t.rect[3] + 2.];
                members.extend(
                    lines
                        .iter()
                        .filter(|l| {
                            contains(frame, center(l.rect()))
                                && l.rect()[0] >= frame[0]
                                && l.rect()[2] <= frame[2]
                        })
                        .cloned(),
                );
                members.push(Line {
                    a: [t.rect[0], top],
                    b: [t.rect[2], top],
                    width: t.border,
                    color: t.color,
                });
                for x in &t.xs {
                    members.push(Line {
                        a: [*x, top],
                        b: [*x, t.rect[1]],
                        width: t.border,
                        color: t.color,
                    });
                }
                detected = grid(&members, &paragraphs, &fills);
            }
        }
        if let Some(table) = detected {
            paragraphs.retain(|p| !contains(table.rect, center(p.rect)));
            items.push(Item::Table(table));
        } else {
            let r = group
                .iter()
                .map(|&i| lines[i].rect())
                .reduce(union)
                .unwrap();
            let heading_frame = r[3] - r[1] < 60.
                && r[2] - r[0] > (r[3] - r[1]) * 4.
                && paragraphs.iter().any(|p| contains(r, center(p.rect)));
            if !heading_frame && r[2] - r[0] > 5. && r[3] - r[1] > 5. {
                artwork.push(r);
            }
        }
    }
    artwork.retain(|r| !items.iter().any(|i| matches!(i,Item::Table(t) if contains(t.rect,[r[0],r[1]]) && contains(t.rect,[r[2],r[3]]))));
    paragraphs = fractions(paragraphs, &lines);
    let mut bitmap_rects = Vec::new();
    for image in pdfcraft_edit::reading_images(&e.doc, index).map_err(|e| e.to_string())? {
        if !image.is_form {
            bitmap_rects.push(view_rect(p, image.visible_rect.unwrap_or(image.rect)));
        }
    }
    // The general paint inventory also contains Bézier curves and compound
    // paths which are intentionally absent from the straight-stroke editor.
    for item in e.selection_items(index)? {
        if !matches!(item.target, crate::selection::Target::Paint(_)) {
            continue;
        }
        let r = view_rect(p, item.rect);
        // A white page-sized fill is paper, not an illustration. Cropping the
        // full rendered page here would flatten text and duplicate real tables.
        if (r[2] - r[0]) * (r[3] - r[1]) >= p.width as f64 * p.height as f64 * 0.9
            && (fills.iter().any(|(rect, color)| {
                color.iter().all(|c| *c >= 0.99)
                    && rect.iter().zip(r).all(|(a, b)| (a - b).abs() <= 1.)
            }) || white_paper(e, index, item.target))
        {
            continue;
        }
        if r[2] - r[0] <= 2. || r[3] - r[1] <= 2. {
            continue;
        }
        if items.iter().any(|item| matches!(item, Item::Table(t) if contains(t.rect, [r[0], r[1]]) && contains(t.rect, [r[2], r[3]]))) { continue; }
        // Keep callouts attached to an editable table out of the text flow.
        // Cropping the rendered page here would copy part of that table into
        // an image and reserve its height for a second time.
        let table_callout = items.iter().any(|i| {
            matches!(i, Item::Table(t) if touches(
                [t.rect[0]-12., t.rect[1]-12., t.rect[2]+12., t.rect[3]+12.], r))
        });
        let text_frame = paragraphs.iter().any(|p| {
            let size = p.runs.iter().map(|r| r.size).fold(0., f64::max);
            contains(r, center(p.rect))
                && r[3] - r[1] <= size * 1.8
                && r[2] - r[0] <= p.rect[2] - p.rect[0] + 24.
        });
        if table_callout || text_frame {
            decoration_targets.push(item.target);
            decoration_regions.push(r);
            continue;
        }
        // A wide, shallow decorative frame around native headings does not
        // turn those headings into an image.
        if r[2] - r[0] > (r[3] - r[1]) * 4.
            && r[3] - r[1] < 60.
            && paragraphs
                .iter()
                .filter(|p| contains(r, center(p.rect)))
                .count()
                >= 1
        {
            decoration_targets.push(item.target);
            decoration_regions.push(r);
            continue;
        }
        artwork.push(r);
    }
    let decoration_bounds: Vec<_> = groups(&decoration_regions)
        .into_iter()
        .filter_map(|g| g.into_iter().map(|i| decoration_regions[i]).reduce(union))
        .collect();
    let is_decoration = |r: &[f64; 4]| {
        decoration_bounds
            .iter()
            .any(|d| contains(*d, [r[0], r[1]]) && contains(*d, [r[2], r[3]]))
    };
    artwork.retain(|r| !is_decoration(r));
    for (image_index, image) in pdfcraft_edit::page_images(&e.doc, index)
        .map_err(|e| e.to_string())?
        .into_iter()
        .enumerate()
    {
        if image.is_form {
            let mut r = view_rect(p, image.visible_rect.unwrap_or(image.rect));
            if (r[2] - r[0]) * (r[3] - r[1]) >= p.width as f64 * p.height as f64 * 0.5
                && image_index < 64
                && let Some(ink) = form_ink_rect(e, index, image_index)
            {
                r = ink;
            }
            if (r[2] - r[0]) * (r[3] - r[1]) < p.width as f64 * p.height as f64 * 0.5 {
                artwork.push(r);
            }
        }
    }
    artwork.retain(|r| !is_decoration(r));
    // Merge neighbouring components of one figure before taking a crop. Labels
    // inside it travel with the figure, avoiding a second copy in the text flow.
    if !artwork.is_empty() || !bitmap_rects.is_empty() {
        let mut renderer = pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
        let raster = renderer.render(pdfcraft_render::RenderRequest {
            page: index,
            scale: 3.,
            ..Default::default()
        });
        if let Some(error) = raster.error {
            return Err(error);
        }
        let sx = raster.width as f64 / p.width as f64;
        let sy = raster.height as f64 / p.height as f64;
        // White bitmap margins are not figure content. Tighten them before
        // grouping, so adjacent illustrations don't absorb nearby prose or
        // force a bottom-of-page figure onto an extra page.
        for r in bitmap_rects {
            let x0 = (r[0] * sx).floor().max(0.) as u32;
            let y0 = (r[1] * sy).floor().max(0.) as u32;
            let x1 = (r[2] * sx).ceil().min(raster.width as f64) as u32;
            let y1 = (r[3] * sy).ceil().min(raster.height as f64) as u32;
            let mut ink = [x1, y1, x0, y0];
            for y in y0..y1 {
                for x in x0..x1 {
                    let pixel = &raster.rgba[((y * raster.width + x) * 4) as usize..][..4];
                    if pixel[3] > 0 && pixel[..3].iter().any(|c| *c < 245) {
                        ink = [
                            ink[0].min(x),
                            ink[1].min(y),
                            ink[2].max(x + 1),
                            ink[3].max(y + 1),
                        ];
                    }
                }
            }
            if ink[2] > ink[0] && ink[3] > ink[1] {
                artwork.push([
                    ink[0] as f64 / sx,
                    ink[1] as f64 / sy,
                    ink[2] as f64 / sx,
                    ink[3] as f64 / sy,
                ]);
            }
        }
        let rects: Vec<_> = figure_groups(&artwork)
            .into_iter()
            .filter_map(|g| g.into_iter().map(|i| artwork[i]).reduce(union))
            .collect();
        for &original in &rects {
            let mut r = original;
            r = [r[0] - 6., r[1] - 12., r[2] + 6., r[3] + 6.];
            // Crop padding from consecutive figures must not overlap: inline
            // flow cannot overlap their white margins without shifting a figure.
            for other in &rects {
                if other[0] < original[2] && original[0] < other[2] {
                    if other[3] < original[1] {
                        r[1] = r[1].max((other[3] + original[1]) / 2. + 0.5);
                    }
                    if original[3] < other[1] {
                        r[3] = r[3].min((original[3] + other[1]) / 2. - 0.5);
                    }
                }
            }
            for text in &paragraphs {
                let b = text.rect;
                if b[3] > original[1] && b[1] < original[3] && b[2] <= original[0] && b[0] < r[0] {
                    r[0] = r[0].max(b[2] + 1.);
                }
            }
            for item in &items {
                if let Item::Table(t) = item {
                    if r[1] < t.rect[3] && r[3] > t.rect[1] && original[0] >= t.rect[2] - 2. {
                        r[0] = r[0].max(t.rect[2] + 2.5);
                    }
                    if t.rect[3] < original[1] {
                        r[1] = r[1].max(t.rect[3] + 1.);
                    }
                    if t.rect[1] > original[3] {
                        r[3] = r[3].min(t.rect[1] - 1.);
                    }
                }
            }
            for paragraph in &paragraphs {
                if contains(r, center(paragraph.rect))
                    || (touches(r, paragraph.rect)
                        && paragraph
                            .runs
                            .iter()
                            .map(|r| r.text.chars().count())
                            .sum::<usize>()
                            <= 30)
                {
                    r = union(r, paragraph.rect);
                }
            }
            r = [
                r[0].max(0.),
                r[1].max(0.),
                r[2].min(p.width as f64),
                r[3].min(p.height as f64),
            ];
            if r[2] <= r[0] || r[3] <= r[1] {
                continue;
            }
            let x0 = (r[0] * sx).floor() as u32;
            let x1 = (r[2] * sx).ceil().min(raster.width as f64) as u32;
            let y0 = (r[1] * sy).floor() as u32;
            let y1 = (r[3] * sy).ceil().min(raster.height as f64) as u32;
            let mut rgba = Vec::new();
            for y in y0..y1 {
                rgba.extend_from_slice(
                    &raster.rgba[((y * raster.width + x0) * 4) as usize
                        ..((y * raster.width + x1) * 4) as usize],
                );
            }
            // The crop's white margin can include the end of a prose line
            // beside an irregular figure. That line stays native in the flow;
            // do not paint its fragment a second time inside the illustration.
            for text in paragraphs.iter().filter(|p| !contains(r, center(p.rect))) {
                let b = text.rect;
                let left = ((b[0] * sx).floor().max(x0 as f64) as u32).min(x1);
                let right = ((b[2] * sx).ceil().max(x0 as f64) as u32).min(x1);
                let top = ((b[1] * sy).floor().max(y0 as f64) as u32).min(y1);
                let bottom = ((b[3] * sy).ceil().max(y0 as f64) as u32).min(y1);
                for y in top..bottom {
                    for x in left..right {
                        rgba[(((y - y0) * (x1 - x0) + (x - x0)) * 4 + 3) as usize] = 0;
                    }
                }
            }
            if !rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] > 0 && p[..3].iter().any(|c| *c < 250))
            {
                continue;
            }
            let bytes = crate::ocr::encode_png(&rgba, x1 - x0, y1 - y0)?;
            paragraphs.retain(|p| !contains(r, center(p.rect)));
            items.push(Item::Picture(Picture { rect: r, bytes }));
        }
    }
    for mut paragraph in join_lines(paragraphs, 1.5) {
        // Source line wraps become ordinary spaces so Word can reflow a paragraph.
        if paragraph.spacing.is_none() {
            for run in &mut paragraph.runs {
                run.text = run.text.replace('\n', " ");
            }
        }
        items.push(Item::Paragraph(paragraph));
    }
    preserve_complex_math(e, index, &mut items)?;
    items.sort_by(|a, b| {
        a.rect()[1]
            .total_cmp(&b.rect()[1])
            .then(a.rect()[0].total_cmp(&b.rect()[0]))
    });
    let bounds = items.iter().map(Item::rect).reduce(union).unwrap_or([
        36.,
        36.,
        p.width as f64 - 36.,
        p.height as f64 - 36.,
    ]);
    // A tight glyph bound is not a Word line limit. Font-unit rounding and
    // kerning differences need a small allowance to avoid an extra wrapped row.
    let max_size = items
        .iter()
        .filter_map(|i| {
            if let Item::Paragraph(p) = i {
                Some(p.runs.iter().map(|r| r.size).fold(1., f64::max))
            } else {
                None
            }
        })
        .fold(1., f64::max);
    let line_slack = (max_size * 0.5).clamp(2., 20.);
    let margin = [
        bounds[0].clamp(0., 54.),
        bounds[1].clamp(0., 54.),
        (p.width as f64 - bounds[2] - line_slack).clamp(0., 54.),
        // Reserve no artificial footer area: tall construction/table cells
        // must be allowed to fit on their source page after font substitution.
        0.,
    ];
    let mut decorations = Vec::new();
    if !decoration_targets.is_empty() {
        let inventory = e.selection_items(index)?;
        let frames: Vec<_> = inventory
            .iter()
            .filter(|i| decoration_targets.contains(&i.target))
            .map(|i| i.rect)
            .collect();
        for item in inventory {
            if matches!(item.target, crate::selection::Target::Paint(_))
                && frames.iter().any(|r| contains(*r, center(item.rect)))
                && !decoration_targets.contains(&item.target)
            {
                decoration_targets.push(item.target);
            }
        }
        let bytes = e.selection_preview(index, &decoration_targets, true, false)?;
        let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
        let raster = renderer.render(pdfcraft_render::RenderRequest {
            page: index,
            scale: 2.,
            ..Default::default()
        });
        if let Some(error) = raster.error {
            return Err(error);
        }
        let mut rgba = raster.rgba.to_vec();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            // The preview contains only ornamental paints; white is paper.
            if pixel[..3].iter().all(|c| *c > 248) {
                pixel[3] = 0;
            }
        }
        decorations.push(Picture {
            rect: [0., 0., p.width as f64, p.height as f64],
            bytes: crate::ocr::encode_png(&rgba, raster.width, raster.height)?,
        });
    }
    // A PDF figure can have an irregular silhouette with prose beside its
    // bounding rectangle. Inline placement would stack those overlapping
    // rectangles and push the rest of the page down. Word/Writer support real
    // page-anchored illustrations; the text remains ordinary flowing paragraphs.
    // Keep image-only pages inline so they retain a real page carrier.
    if items.iter().any(|i| !matches!(i, Item::Picture(_))) {
        items.retain(|item| {
            if let Item::Picture(picture) = item {
                decorations.push(picture.clone());
                false
            } else {
                true
            }
        });
    }
    let fonts = super::fonts::preserve(e, index, &mut items)?;
    Ok(Page {
        width: p.width as f64,
        height: p.height as f64,
        margin,
        items,
        decorations,
        fonts,
    })
}

#[cfg(test)]
mod fraction_tests {
    use super::*;
    #[test]
    fn simple_fraction_stays_native_while_complex_math_requires_source_preservation() {
        let small = sample("1", false).remove(0);
        assert!(!complex_math(&small, false));
        assert!(complex_math(&small, true));
        assert!(complex_math(&sample("123", false).remove(0), false));
    }
    #[test]
    fn preserved_math_pixels_match_source_and_prose_and_tables_stay_native() {
        use pdfcraft_cos::{Dict, Object, Stream};
        let mut doc = pdfcraft_create::blank(200., 200., 1).unwrap();
        let mut font = Dict::new();
        font.set(b"Type".to_vec(), Object::name("Font"));
        font.set(b"Subtype".to_vec(), Object::name("Type1"));
        font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
        let mut fonts = Dict::new();
        fonts.set(b"F1".to_vec(), Object::Ref(doc.add(font)));
        let mut resources = Dict::new();
        resources.set(b"Font".to_vec(), Object::Dict(fonts));
        let stream=doc.add(Stream::flate(Dict::new(),b"BT /F1 12 Tf 20 170 Td (Editable prose) Tj ET BT /F1 12 Tf 20 130 Td (123) Tj ET BT /F1 12 Tf 20 100 Td (4) Tj ET 19 125 m 44 125 l S"));
        let page = pdfcraft_model::pages(&doc)[0].obj;
        doc.update_dict(page, |p| {
            p.set(b"Resources".to_vec(), Object::Dict(resources));
            p.set(b"Contents".to_vec(), Object::Ref(stream));
        })
        .unwrap();
        let editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
                .unwrap();
        let before = editor.bytes.clone();
        let mut math = sample("123", false).remove(0);
        math.rect = [18., 58., 50., 105.];
        math.runs[0].rect = Some(math.rect);
        if let Some(f) = &mut math.runs[0].fraction {
            f.numerator[0].rect = Some([20., 58., 40., 70.]);
            f.denominator[0].rect = Some([20., 88., 26., 100.]);
        }
        let mut items = vec![
            Item::Paragraph(paragraph("Editable prose", [20., 18., 130., 30.])),
            Item::Paragraph(math),
            Item::Table(Table {
                rect: [100., 100., 180., 180.],
                xs: vec![],
                ys: vec![],
                rows: vec![],
                border: 1.,
                color: [0.; 3],
            }),
        ];
        preserve_complex_math(&editor, 0, &mut items).unwrap();
        assert_eq!(
            items
                .iter()
                .filter(|i| matches!(i, Item::Paragraph(_)))
                .count(),
            1
        );
        assert!(items.iter().any(|i| matches!(i, Item::Table(_))));
        let picture = items
            .iter()
            .find_map(|i| {
                if let Item::Picture(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&picture.bytes))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let output = reader.next_frame(&mut rgba).unwrap();
        let mut renderer =
            pdfcraft_render::PageRenderer::new(editor.bytes.clone(), Default::default());
        let source = renderer.render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 3.,
            ..Default::default()
        });
        let x = (picture.rect[0] * 3.).floor() as u32;
        let y = (picture.rect[1] * 3.).floor() as u32;
        for row in 0..output.height {
            let start = (((y + row) * source.width + x) * 4) as usize;
            assert_eq!(
                &rgba[(row * output.width * 4) as usize..((row + 1) * output.width * 4) as usize],
                &source.rgba[start..start + (output.width * 4) as usize]
            );
        }
        assert_eq!(before, editor.bytes);
    }
    #[test]
    fn neighbouring_figure_parts_stay_together_without_joining_distant_figures() {
        let rects = [
            [20., 40., 60., 100.],
            [70., 30., 80., 120.],
            [180., 40., 220., 100.],
        ];
        assert_eq!(figure_groups(&rects), vec![vec![0, 1], vec![2]]);
    }
    #[test]
    fn oversized_form_bbox_uses_its_visible_ink_for_export() {
        use pdfcraft_cos::{Dict, Object, Stream};
        let mut doc = pdfcraft_create::blank(200., 200., 1).unwrap();
        let mut form = Dict::new();
        form.set(b"Type".to_vec(), Object::name("XObject"));
        form.set(b"Subtype".to_vec(), Object::name("Form"));
        form.set(
            b"BBox".to_vec(),
            Object::Array(vec![0.into(), 0.into(), 1000.into(), 1000.into()]),
        );
        let object = doc.add(Stream::flate(form, b"0 G 2 w 20 20 m 80 20 l 50 80 l h S"));
        let mut xobjects = Dict::new();
        xobjects.set(b"Figure".to_vec(), Object::Ref(object));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
        let stream = doc.add(Stream::flate(Dict::new(), b"/Figure Do"));
        let page = pdfcraft_model::pages(&doc)[0].obj;
        doc.update_dict(page, |d| {
            d.set(b"Resources".to_vec(), Object::Dict(resources));
            d.set(b"Contents".to_vec(), Object::Ref(stream));
        })
        .unwrap();
        let editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
                .unwrap();
        let rect = form_ink_rect(&editor, 0, 0).unwrap();
        assert!(rect[0] < 21. && rect[0] > 16. && rect[2] > 79. && rect[2] < 84.);
        assert!(rect[1] > 116. && rect[3] < 184.);
        let page = super::page(&editor, 0).unwrap();
        assert!(
            page.items
                .iter()
                .any(|i| matches!(i, Item::Picture(p) if p.rect[2]-p.rect[0] < 100.))
        );
    }
    #[test]
    fn missing_header_geometry_requires_visible_borders_in_the_pdf() {
        use pdfcraft_cos::{Dict, Object, Stream};
        let mut doc = pdfcraft_create::blank(200., 200., 1).unwrap();
        let page = pdfcraft_model::pages(&doc)[0].obj;
        let stream=doc.add(Stream::flate(Dict::new(),b"0 G 1 w 20 50 160 80 re S 20 90 m 180 90 l S 60 50 m 60 130 l S 100 50 m 100 130 l S 140 50 m 140 130 l S"));
        doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(stream)))
            .unwrap();
        let editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
                .unwrap();
        let table = Table {
            rect: [20., 110., 180., 150.],
            xs: vec![20., 60., 100., 140., 180.],
            ys: vec![110., 150.],
            rows: vec![],
            border: 1.,
            color: [0.; 3],
        };
        assert!(visible_header(
            &editor,
            0,
            &editor.info.pages[0],
            &table,
            70.
        ));
        let blank = Editor::from_bytes(
            pdfcraft_cos::write_full(
                &pdfcraft_create::blank(200., 200., 1).unwrap(),
                &Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!visible_header(
            &blank,
            0,
            &blank.info.pages[0],
            &table,
            70.
        ));
    }
    #[test]
    fn raised_glyph_arriving_before_separate_base_words_is_reattached() {
        let mut words = vec![
            paragraph("Classe ", [0., 20., 30., 32.]),
            paragraph("la ", [35., 20., 60., 32.]),
            paragraph("4", [65., 20., 90., 32.]),
        ];
        for p in &mut words {
            p.runs[0].baseline = Some(29.);
        }
        let mut script = paragraph("e", [90., 13., 95., 19.]);
        script.runs[0].size = 8.;
        script.runs[0].baseline = Some(21.);
        words.push(script);
        let joined = join_lines(words, 1.5);
        assert_eq!(joined.len(), 1);
        assert_eq!(
            joined[0]
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "Classe la 4e"
        );
    }
    #[test]
    fn footer_superscript_joins_its_base_without_becoming_an_extra_line() {
        let mut base = paragraph("Classe 4", [0., 20., 45., 32.]);
        base.runs[0].baseline = Some(29.);
        let mut suffix = paragraph(" suite", [52., 20., 100., 32.]);
        suffix.runs[0].baseline = Some(29.);
        base.runs.extend(suffix.runs);
        base.rect[2] = 100.;
        let mut script = paragraph("e", [46., 13., 51., 19.]);
        script.runs[0].size = 8.;
        script.runs[0].baseline = Some(21.);
        let joined = join_lines(vec![base, script], 1.5);
        assert_eq!(joined.len(), 1);
        assert_eq!(
            joined[0]
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "Classe 4e suite"
        );
        assert_eq!(joined[0].runs[1].rise, Some(8.));
    }
    fn paragraph(text: &str, rect: [f64; 4]) -> Paragraph {
        Paragraph {
            rect,
            align: "left",
            spacing: None,
            runs: vec![Run {
                text: text.into(),
                font: "Arial".into(),
                size: 12.,
                color: [0.; 3],
                bold: false,
                italic: false,
                underline: false,
                strike: false,
                script: 0,
                baseline: None,
                rise: None,
                background: None,
                rect: Some(rect),
                fraction: None,
            }],
        }
    }
    fn sample(num: &str, diagonal: bool) -> Vec<Paragraph> {
        fractions(
            vec![
                paragraph(num, [80., 30., 102., 42.]),
                paragraph("7", [87., 50., 94., 62.]),
            ],
            &[Line {
                a: [78., 45.],
                b: [112., if diagonal { 60. } else { 45. }],
                width: 1.,
                color: [0.; 3],
            }],
        )
    }
    #[test]
    fn fraction_bounds_include_the_whole_source_bar() {
        let p = sample("12", false);
        let r = p
            .iter()
            .flat_map(|p| &p.runs)
            .find(|r| r.fraction.is_some())
            .unwrap();
        assert_eq!(r.rect.unwrap(), [78., 30., 112., 62.]);
    }
    #[test]
    fn unicode_minus_and_common_operators_keep_editable_fractions() {
        for text in ["−12", "±12", "∓12", "1⋅2", "1·2", "1×2", "1÷2"] {
            let p = sample(text, false);
            assert_eq!(
                p.iter()
                    .flat_map(|p| &p.runs)
                    .filter(|r| r.fraction.is_some())
                    .count(),
                1,
                "{text}"
            );
            let r = p
                .iter()
                .flat_map(|p| &p.runs)
                .find(|r| r.fraction.is_some())
                .unwrap();
            assert_eq!(r.fraction.as_ref().unwrap().numerator[0].text, text);
        }
    }
    #[test]
    fn prose_and_diagonal_strokes_do_not_turn_into_fractions() {
        for p in [sample("Chapitre", false), sample("12", true)] {
            assert!(p.iter().flat_map(|p| &p.runs).all(|r| r.fraction.is_none()));
        }
    }
}
