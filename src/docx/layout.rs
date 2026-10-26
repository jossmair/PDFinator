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
pub(super) fn font(name: &str) -> String {
    let name = name.split_once('+').map_or(name, |(_, n)| n);
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
            .trim()
            .to_string()
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
            overlap > (bounds[3] - bounds[1]).min(p.rect[3] - p.rect[1]) * 0.35
                && (p.rect[0] - bounds[2]).max(bounds[0] - p.rect[2]) < gap * size
        });
        if let Some(row) = row {
            row.push(p);
        } else {
            rows.push(vec![p]);
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.rect[0].total_cmp(&b.rect[0]));
            let max_size = row
                .iter()
                .flat_map(|p| p.runs.iter())
                .filter(|r| r.fraction.is_none())
                .map(|r| r.size)
                .fold(1_f64, f64::max);
            let baseline = row
                .iter()
                .flat_map(|p| &p.runs)
                .filter(|r| r.fraction.is_none())
                .filter_map(|r| r.rect.map(|b| b[3]))
                .fold(0_f64, f64::max);
            for p in &mut row {
                if p.rect[3] < baseline - max_size * 0.2 {
                    for r in &mut p.runs {
                        if r.fraction.is_none() && r.size < max_size * 0.8 {
                            r.script = 1;
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
    for line in lines.iter().filter(|l| !l.vertical()) {
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
            if lines.iter().any(|l| {
                let r = l.rect();
                !l.vertical()
                    && (r[1] - bounds[3]).abs() < 2.5
                    && r[0] <= bounds[0] + 2.
                    && r[2] >= bounds[2] - 2.
                    && r[2] - r[0] < paragraph.rect[2] - paragraph.rect[0] + 12.
            }) {
                run.underline = true;
            }
        }
    }
    let mut items = Vec::new();
    for group in groups(&lines.iter().map(Line::rect).collect::<Vec<_>>()) {
        let members: Vec<_> = group.iter().map(|&i| lines[i].clone()).collect();
        if let Some(table) = grid(&members, &paragraphs, &fills) {
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
        if r[2] - r[0] <= 2. || r[3] - r[1] <= 2. {
            continue;
        }
        if items.iter().any(|item| matches!(item, Item::Table(t) if contains(t.rect, [r[0], r[1]]) && contains(t.rect, [r[2], r[3]]))) { continue; }
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
            continue;
        }
        artwork.push(r);
    }
    for image in pdfcraft_edit::page_images(&e.doc, index).map_err(|e| e.to_string())? {
        if image.is_form {
            let r = view_rect(p, image.visible_rect.unwrap_or(image.rect));
            if (r[2] - r[0]) * (r[3] - r[1]) < p.width as f64 * p.height as f64 * 0.5 {
                artwork.push(r);
            }
        }
    }
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
        let rects: Vec<_> = groups(&artwork)
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
    let margin = [
        bounds[0].clamp(0., 54.),
        bounds[1].clamp(0., 54.),
        (p.width as f64 - bounds[2]).clamp(0., 54.),
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
