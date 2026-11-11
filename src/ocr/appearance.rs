//! Recover text size and weight from local font outlines and the source ink.
use super::{Block, Line};

#[derive(Default)]
struct Area {
    sum: f64,
    first: (f64, f64),
    last: (f64, f64),
}
impl Area {
    fn edge(&mut self, p: (f64, f64)) {
        self.sum += (self.last.0 * p.1 - p.0 * self.last.1) * 0.5;
        self.last = p;
    }
}
impl ttf_parser::OutlineBuilder for Area {
    fn move_to(&mut self, x: f32, y: f32) {
        self.first = (x as f64, y as f64);
        self.last = self.first;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.edge((x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p = self.last;
        for i in 1..=12 {
            let t = i as f64 / 12.;
            let u = 1. - t;
            self.edge((
                u * u * p.0 + 2. * u * t * x1 as f64 + t * t * x as f64,
                u * u * p.1 + 2. * u * t * y1 as f64 + t * t * y as f64,
            ));
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p = self.last;
        for i in 1..=16 {
            let t = i as f64 / 16.;
            let u = 1. - t;
            self.edge((
                u * u * u * p.0
                    + 3. * u * u * t * x1 as f64
                    + 3. * u * t * t * x2 as f64
                    + t * t * t * x as f64,
                u * u * u * p.1
                    + 3. * u * u * t * y1 as f64
                    + 3. * u * t * t * y2 as f64
                    + t * t * t * y as f64,
            ));
        }
    }
    fn close(&mut self) {
        self.edge(self.first);
    }
}
struct Metrics {
    rect: [f64; 4],
    area: f64,
}
fn metrics(face: &ttf_parser::Face<'_>, text: &str) -> Option<Metrics> {
    let mut pen = 0.;
    let mut rect = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut area = 0.;
    for c in text.chars() {
        let id = face.glyph_index(c)?;
        if let Some(b) = face.glyph_bounding_box(id) {
            rect[0] = rect[0].min(pen + b.x_min as f64);
            rect[2] = rect[2].max(pen + b.x_max as f64);
            rect[1] = rect[1].min(b.y_min as f64);
            rect[3] = rect[3].max(b.y_max as f64);
            let mut shape = Area::default();
            face.outline_glyph(id, &mut shape)?;
            area += shape.sum.abs();
        }
        pen += face.glyph_hor_advance(id)? as f64;
    }
    let em = face.units_per_em() as f64;
    (area > 0. && rect.iter().all(|v| v.is_finite())).then_some(Metrics {
        rect: rect.map(|v| v / em),
        area: area / (em * em),
    })
}
fn ink(rgba: &[u8], width: u32, height: u32, r: [f32; 4]) -> Option<Metrics> {
    if r.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let x0 = r[0].floor().max(0.) as u32;
    let x1 = r[2].ceil().min(width as f32).max(0.) as u32;
    let y0 = r[1].floor().max(0.) as u32;
    let y1 = r[3].ceil().min(height as f32).max(0.) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let luma = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        rgba[i] as f64 * 0.2126 + rgba[i + 1] as f64 * 0.7152 + rgba[i + 2] as f64 * 0.0722
    };
    let mut border = Vec::new();
    for y in y0.saturating_sub(2)..(y1 + 2).min(height) {
        for x in x0.saturating_sub(2)..(x1 + 2).min(width) {
            if x < x0 || x >= x1 || y < y0 || y >= y1 {
                border.push(luma(x, y));
            }
        }
    }
    if border.is_empty() {
        return None;
    }
    border.sort_by(f64::total_cmp);
    let background = border[border.len() / 2];
    let mut darkest = background;
    for y in y0..y1 {
        for x in x0..x1 {
            darkest = darkest.min(luma(x, y));
        }
    }
    let contrast = background - darkest;
    if contrast < 40. {
        return None;
    }
    let mut rect = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut area = 0.;
    for y in y0..y1 {
        for x in x0..x1 {
            let coverage = ((background - luma(x, y)) / contrast).clamp(0., 1.);
            area += coverage;
            if coverage > 0.35 {
                rect[0] = rect[0].min(x as f64);
                rect[1] = rect[1].min(y as f64);
                rect[2] = rect[2].max(x as f64 + 1.);
                rect[3] = rect[3].max(y as f64 + 1.);
            }
        }
    }
    (area > 2. && rect.iter().all(|v| v.is_finite())).then_some(Metrics { rect, area })
}
/// Uses installed Arial only; missing fonts keep the conservative recognition defaults.
pub(super) fn refine(rgba: &[u8], width: u32, height: u32, lines: &[Line], blocks: &mut [Block]) {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || rgba.len() != width as usize * height as usize * 4
    {
        return;
    }
    let Some(windows) = std::env::var_os("WINDIR") else {
        return;
    };
    let base = std::path::PathBuf::from(windows).join("Fonts");
    let Ok(regular) = std::fs::read(base.join("arial.ttf")) else {
        return;
    };
    let Ok(bold) = std::fs::read(base.join("arialbd.ttf")) else {
        return;
    };
    let Ok(regular) = ttf_parser::Face::parse(&regular, 0) else {
        return;
    };
    let Ok(bold) = ttf_parser::Face::parse(&bold, 0) else {
        return;
    };
    let context = blocks.to_vec();
    for block in blocks {
        let continuation = context.iter().find(|other| {
            let (first, second) = if block.text.ends_with('-') {
                (&*block, *other)
            } else if other.text.ends_with('-') {
                (*other, &*block)
            } else {
                return false;
            };
            let gap = second.rect[1] - first.rect[3];
            let height = (first.rect[3] - first.rect[1]).max(second.rect[3] - second.rect[1]);
            let centers =
                ((first.rect[0] + first.rect[2]) - (second.rect[0] + second.rect[2])).abs() * 0.5;
            first
                .text
                .trim_end_matches('-')
                .chars()
                .last()
                .is_some_and(char::is_alphabetic)
                && second.text.chars().next().is_some_and(char::is_alphabetic)
                && gap >= -2.
                && gap < height
                && centers < height
        });
        let mut candidates = [Vec::new(), Vec::new()];
        let mut scores = [0., 0.];
        let mut weights = [0., 0.];
        for rect in block
            .words
            .iter()
            .chain(continuation.into_iter().flat_map(|b| &b.words))
        {
            let Some(word) = lines
                .iter()
                .flat_map(|l| &l.words)
                .find(|w| w.rect == *rect)
            else {
                continue;
            };
            let Some(actual) = ink(rgba, width, height, *rect) else {
                continue;
            };
            for (index, face) in [&regular, &bold].iter().enumerate() {
                let Some(expected) = metrics(face, &word.text) else {
                    continue;
                };
                let size = (actual.rect[3] - actual.rect[1])
                    / (expected.rect[3] - expected.rect[1]).max(0.1);
                let predicted_width = (expected.rect[2] - expected.rect[0]) * size;
                let score = (actual.area / (expected.area * size * size)).ln().abs()
                    + ((actual.rect[2] - actual.rect[0]) / predicted_width.max(1.))
                        .ln()
                        .abs();
                let weight = word.text.chars().count().min(12) as f64;
                scores[index] += score * weight;
                weights[index] += weight;
                let width_size = (actual.rect[2] - actual.rect[0])
                    / (expected.rect[2] - expected.rect[0]).max(0.1);
                let fitted = if word.text.chars().count() >= 3
                    && (0.8..1.2).contains(&(width_size / size))
                {
                    size * 0.25 + width_size * 0.75
                } else {
                    size
                };
                candidates[index].push((fitted, weight));
            }
        }
        if weights[0] == 0. || weights[1] == 0. {
            continue;
        }
        let is_bold = scores[1] / weights[1] + 0.1 < scores[0] / weights[0];
        let index = usize::from(is_bold);
        if scores[index] / weights[index] > 0.8 {
            continue;
        }
        let values = &mut candidates[index];
        values.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut sum = 0.;
        let mut size = block.size;
        for &(value, weight) in values.iter() {
            sum += weight;
            if sum >= weights[index] * 0.5 {
                size = value;
                break;
            }
        }
        let Some(expected) = metrics(if is_bold { &bold } else { &regular }, &block.text) else {
            continue;
        };
        block.size = size.clamp(3., 256.);
        block.bold = is_bold;
        // The PDF text writer places its baseline 0.95 em below the frame top.
        block.rect[0] -= (expected.rect[0] * block.size) as f32;
        block.rect[1] += ((expected.rect[3] - 0.95) * block.size) as f32;
        let advance =
            pdfcraft_edit::Family::Helvetica.width(&block.text, block.size, block.bold, false)
                as f32;
        block.rect[2] = block.rect[2].max(block.rect[0] + advance + 2.);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::ocr::{Recognition, Word};
    fn reference() -> (Vec<u8>, u32, u32, Recognition) {
        let labels = [
            ("Quadrilatère", 22., true, 210.),
            ("qui a", 14., false, 155.),
            ("un angle droit", 14., false, 120.),
            ("Parallélo-", 22., true, 80.),
            ("gramme", 22., true, 50.),
        ];
        let mut doc = pdfcraft_create::blank(320., 230., 1).unwrap();
        for (text, size, bold, top) in labels {
            pdfcraft_edit::add_content(
                &mut doc,
                0,
                &pdfcraft_edit::Content::Text(pdfcraft_edit::AddedText {
                    rect: [20., top - 35., 300., top],
                    text: text.into(),
                    size,
                    bold,
                    ..Default::default()
                }),
            )
            .unwrap();
        }
        let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).unwrap();
        let mut renderer =
            pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes), Default::default());
        let image = renderer.render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 2.,
            ..Default::default()
        });
        assert!(image.error.is_none());
        let mut lines = Vec::new();
        for (text, size, bold, top) in labels {
            let mut words = Vec::new();
            let mut x = 40.;
            for word in text.split_whitespace() {
                let advance = pdfcraft_edit::Family::Helvetica.width(word, size, bold, false) * 2.;
                let r = [
                    x as f32,
                    ((230. - top) * 2.) as f32,
                    (x + advance + 1.) as f32,
                    ((230. - top + size * 1.3) * 2.) as f32,
                ];
                let actual = ink(&image.rgba, image.width, image.height, r).unwrap();
                words.push(Word {
                    text: word.into(),
                    rect: actual.rect.map(|v| v as f32),
                });
                x += advance + pdfcraft_edit::Family::Helvetica.width(" ", size, bold, false) * 2.;
            }
            let rect = [
                words.first().unwrap().rect[0],
                words
                    .iter()
                    .map(|w| w.rect[1])
                    .fold(f32::INFINITY, f32::min),
                words.last().unwrap().rect[2],
                words.iter().map(|w| w.rect[3]).fold(0., f32::max),
            ];
            lines.push(Line {
                text: text.into(),
                rect,
                words,
            });
        }
        (
            image.rgba.to_vec(),
            image.width,
            image.height,
            Recognition {
                language: "fr-FR".into(),
                lines,
            },
        )
    }
    #[test]
    fn original_font_size_and_bold_survive_short_lowercase_words_and_hyphenated_titles() {
        let (rgba, w, h, result) = reference();
        let blocks = result.styled_blocks(&rgba, w, h);
        assert_eq!(blocks.len(), 5);
        for (index, expected) in [44., 28., 28., 44., 44.].iter().enumerate() {
            assert!(
                (blocks[index].size - expected).abs() < expected * 0.12,
                "{}: {} vs {}",
                blocks[index].text,
                blocks[index].size,
                expected
            );
            assert_eq!(
                blocks[index].bold,
                matches!(index, 0 | 3 | 4),
                "{}",
                blocks[index].text
            );
        }
        assert!(
            (blocks[1].size - blocks[2].size).abs() < 2.,
            "Lowercase x-height must not shrink the second label"
        );
    }
    #[test]
    fn calibrated_text_is_drawn_back_at_its_original_ink_position() {
        let (rgba, w, h, result) = reference();
        let mut blocks = result.styled_blocks(&rgba, w, h);
        let background = crate::ocr::erase_letters(&rgba, w, h, &mut blocks).unwrap();
        let png = crate::ocr::encode_png(&background, w, h).unwrap();
        let doc = pdfcraft_create::blank(w as f64, h as f64, 1).unwrap();
        let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).unwrap();
        let mut editor = crate::document::Editor::from_bytes(bytes).unwrap();
        editor
            .image_bytes(
                0,
                [0., 0., w as f64, h as f64],
                "background.png",
                &png,
                None,
            )
            .unwrap();
        for b in blocks {
            editor
                .add_text(
                    0,
                    pdfcraft_edit::AddedText {
                        rect: [
                            b.rect[0] as f64,
                            h as f64 - b.rect[3] as f64,
                            b.rect[2] as f64 + 4.,
                            h as f64 - b.rect[1] as f64,
                        ],
                        text: b.text,
                        size: b.size,
                        bold: b.bold,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let mut renderer = pdfcraft_render::PageRenderer::new(editor.bytes, Default::default());
        let image = renderer.render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 1.,
            ..Default::default()
        });
        assert!(image.error.is_none());
        for line in result.lines {
            let r = [
                line.rect[0] - 8.,
                line.rect[1] - 8.,
                line.rect[2] + 8.,
                line.rect[3] + 8.,
            ];
            let drawn = ink(&image.rgba, w, h, r).unwrap();
            assert!(
                (drawn.rect[0] - line.rect[0] as f64).abs() <= 3.,
                "{}: left",
                line.text
            );
            assert!(
                (drawn.rect[1] - line.rect[1] as f64).abs() <= 3.,
                "{}: top {} vs {}",
                line.text,
                drawn.rect[1],
                line.rect[1]
            );
            assert!(
                (drawn.rect[2] - line.rect[2] as f64).abs() <= 8.,
                "{}: right",
                line.text
            );
        }
    }
}
