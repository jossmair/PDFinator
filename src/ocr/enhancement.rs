//! A second local recognition pass for faint scans and small type. Coordinates
//! always return to the original image; the enhanced pixels are never saved.
use super::{Line, Recognition};

pub(super) fn prepare(
    rgba: &[u8],
    width: u32,
    height: u32,
    result: &Recognition,
) -> (Vec<u8>, u32, u32) {
    let mut heights = result
        .lines
        .iter()
        .flat_map(|l| &l.words)
        .map(|w| w.rect[3] - w.rect[1])
        .filter(|h| *h >= 4.)
        .collect::<Vec<_>>();
    heights.sort_by(f32::total_cmp);
    let typical = heights.get(heights.len() / 2).copied().unwrap_or(16.);
    let scale = (36. / typical)
        .clamp(1., 2.)
        .min(4096. / width.max(height) as f32);
    let ow = (width as f32 * scale).round().clamp(1., 4096.) as u32;
    let oh = (height as f32 * scale).round().clamp(1., 4096.) as u32;
    let gray = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            // Composite transparent scans on paper before estimating local contrast.
            let lum = (p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) / 256;
            ((lum * p[3] as u32 + 255 * (255 - p[3] as u32)) / 255) as u8
        })
        .collect::<Vec<_>>();
    // Overlapping neighborhoods avoid seams at tile boundaries. Percentiles
    // reject single specks while bringing pale ink and uneven paper into range.
    let cols = width.div_ceil(64) as usize;
    let rows = height.div_ceil(64) as usize;
    let mut levels = Vec::with_capacity(cols * rows);
    for ty in 0..rows {
        for tx in 0..cols {
            let mut histogram = [0u32; 256];
            let mut count = 0;
            for y in (ty * 64).saturating_sub(32)..((ty + 1) * 64 + 32).min(height as usize) {
                for x in (tx * 64).saturating_sub(32)..((tx + 1) * 64 + 32).min(width as usize) {
                    histogram[gray[y * width as usize + x] as usize] += 1;
                    count += 1;
                }
            }
            let percentile = |fraction: u32| {
                let mut n = 0;
                for (value, &amount) in histogram.iter().enumerate() {
                    n += amount;
                    if n * 100 >= count * fraction {
                        return value as f32;
                    }
                }
                255.
            };
            let white = percentile(90);
            let black = percentile(2).min(white - 12.);
            levels.push((black, white));
        }
    }
    let mut normalized = Vec::with_capacity(gray.len());
    for y in 0..height as usize {
        for x in 0..width as usize {
            let (black, white) = levels[(y / 64).min(rows - 1) * cols + (x / 64).min(cols - 1)];
            normalized.push(
                ((gray[y * width as usize + x] as f32 - black) * 255. / (white - black))
                    .clamp(0., 255.) as u8,
            );
        }
    }
    let mut output = vec![255; ow as usize * oh as usize * 4];
    for y in 0..oh {
        for x in 0..ow {
            let sx = ((x as f32 + 0.5) * width as f32 / ow as f32 - 0.5).max(0.);
            let sy = ((y as f32 + 0.5) * height as f32 / oh as f32 - 0.5).max(0.);
            let (ix, iy) = (sx as u32, sy as u32);
            let at = |dx: u32, dy: u32| {
                normalized[((iy + dy).min(height - 1) * width + (ix + dx).min(width - 1)) as usize]
                    as f32
            };
            let a = at(0, 0) * (1. - sx.fract()) + at(1, 0) * sx.fract();
            let b = at(0, 1) * (1. - sx.fract()) + at(1, 1) * sx.fract();
            let lum = (a * (1. - sy.fract()) + b * sy.fract()).round() as u8;
            let i = ((y * ow + x) * 4) as usize;
            output[i..i + 3].fill(lum);
        }
    }
    (output, ow, oh)
}

pub(super) fn merge(first: &mut Recognition, second: Recognition, sx: f32, sy: f32) {
    let area = |r: [f32; 4]| (r[2] - r[0]).max(0.) * (r[3] - r[1]).max(0.);
    let mut occupied = first
        .lines
        .iter()
        .flat_map(|l| &l.words)
        .map(|w| w.rect)
        .collect::<Vec<_>>();
    for line in second.lines {
        let mut words = Vec::new();
        for mut word in line.words {
            word.rect = [
                word.rect[0] * sx,
                word.rect[1] * sy,
                word.rect[2] * sx,
                word.rect[3] * sy,
            ];
            if occupied.iter().any(|r| {
                let overlap = [
                    r[0].max(word.rect[0]),
                    r[1].max(word.rect[1]),
                    r[2].min(word.rect[2]),
                    r[3].min(word.rect[3]),
                ];
                area(overlap) > 0.2 * area(*r).min(area(word.rect))
            }) {
                continue;
            }
            occupied.push(word.rect);
            words.push(word);
        }
        if words.is_empty() {
            continue;
        }
        let rect = words.iter().fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |r, w| {
                [
                    r[0].min(w.rect[0]),
                    r[1].min(w.rect[1]),
                    r[2].max(w.rect[2]),
                    r[3].max(w.rect[3]),
                ]
            },
        );
        let text = words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        first.lines.push(Line { text, rect, words });
    }
}

/// A tiny raised ring after digits is a degree sign, not a baseline zero. Check
/// the actual disconnected ink and its enclosed white center before correcting.
pub(super) fn repair_degrees(rgba: &[u8], width: u32, height: u32, result: &mut Recognition) {
    for line in &mut result.lines {
        for word in &mut line.words {
            let Some(prefix) = word.text.strip_suffix(['0', 'o', 'O']) else {
                continue;
            };
            if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let r = [
                word.rect[0].floor().max(0.) as u32,
                word.rect[1].floor().max(0.) as u32,
                word.rect[2].ceil().min(width as f32) as u32,
                word.rect[3].ceil().min(height as f32) as u32,
            ];
            let (w, h) = ((r[2] - r[0]) as usize, (r[3] - r[1]) as usize);
            if w == 0 || h < 8 {
                continue;
            }
            let dark = |x: usize, y: usize| {
                let i = ((r[1] as usize + y) * width as usize + r[0] as usize + x) * 4;
                rgba[i..i + 3].iter().map(|v| *v as u32).sum::<u32>() < 570
            };
            let mut visited = vec![false; w * h];
            let mut components = Vec::new();
            for at in 0..w * h {
                if visited[at] || !dark(at % w, at / w) {
                    continue;
                }
                visited[at] = true;
                let mut points = vec![at];
                let mut b = [at % w, at / w, at % w + 1, at / w + 1];
                let mut cursor = 0;
                while cursor < points.len() {
                    let (x, y) = (points[cursor] % w, points[cursor] / w);
                    cursor += 1;
                    b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
                    for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                            let i = ny * w + nx;
                            if !visited[i] && dark(nx, ny) {
                                visited[i] = true;
                                points.push(i);
                            }
                        }
                    }
                }
                if points.len() >= 4 {
                    components.push(b);
                }
            }
            let Some(b) = components.iter().max_by_key(|b| b[2]) else {
                continue;
            };
            let (cw, ch) = (b[2] - b[0], b[3] - b[1]);
            let main_height = components.iter().map(|b| b[3] - b[1]).max().unwrap_or(0);
            if cw < 3
                || ch < 3
                || ch * 10 > main_height * 7
                || cw * 10 > main_height * 8
                || b[3] * 10 > h * 7
                || cw > ch * 2
                || ch > cw * 2
            {
                continue;
            }
            let (cx, cy) = ((b[0] + b[2]) / 2, (b[1] + b[3]) / 2);
            // Four sides enclose a light center, excluding accents and specks.
            if !dark(cx, cy)
                && (b[0]..cx).any(|x| dark(x, cy))
                && (cx + 1..b[2]).any(|x| dark(x, cy))
                && (b[1]..cy).any(|y| dark(cx, y))
                && (cy + 1..b[3]).any(|y| dark(cx, y))
            {
                word.text = format!("{prefix}°");
            }
        }
        line.text = line
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::Word;
    #[cfg(windows)]
    use crate::ocr::windows_recognize;
    #[test]
    fn degree_repair_uses_raised_ring_pixels_and_keeps_real_baseline_zero() {
        for raised in [false, true] {
            let (w, h) = (40, 24);
            let mut pixels = vec![255; w * h * 4];
            for (x0, y0, x1, y1) in if raised {
                vec![(1, 4, 10, 24), (14, 4, 23, 24), (28, 1, 35, 8)]
            } else {
                vec![(1, 4, 10, 24), (14, 4, 23, 24), (28, 4, 37, 24)]
            } {
                for y in y0..y1 {
                    for x in x0..x1 {
                        if x == x0 || x == x1 - 1 || y == y0 || y == y1 - 1 {
                            pixels[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(0);
                        }
                    }
                }
            }
            let rect = [0., 0., 40., 24.];
            let mut result = Recognition {
                language: "fr-FR".into(),
                lines: vec![Line {
                    text: "360".into(),
                    rect,
                    words: vec![Word {
                        text: "360".into(),
                        rect,
                    }],
                }],
            };
            repair_degrees(&pixels, w as u32, h as u32, &mut result);
            assert_eq!(result.text(), if raised { "36°" } else { "360" });
        }
    }
    #[test]
    fn second_pass_adds_missing_words_at_original_coordinates_without_duplicates() {
        let word = |text: &str, x| Word {
            text: text.into(),
            rect: [x, 5., x + 10., 20.],
        };
        let mut first = Recognition {
            language: "fr-FR".into(),
            lines: vec![Line {
                text: "Paris".into(),
                rect: [10., 5., 20., 20.],
                words: vec![word("Paris", 10.)],
            }],
        };
        let mut second = first.clone();
        second.lines[0].words.push(word("Lyon", 50.));
        for w in &mut second.lines[0].words {
            w.rect = w.rect.map(|v| v * 2.);
        }
        merge(&mut first, second, 0.5, 0.5);
        assert_eq!(first.text(), "Paris\nLyon");
        assert_eq!(first.lines[1].words[0].rect, [50., 5., 60., 20.]);
    }
    #[test]
    #[cfg(windows)]
    fn faint_french_scan_with_uneven_paper_keeps_accents_and_numbers() {
        let d = pdfcraft_create::from_text("", "Été français 2026", (420., 160.), 20.).unwrap();
        let bytes = pdfcraft_cos::write_full(&d, &Default::default()).unwrap();
        let mut renderer =
            pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes), Default::default());
        let image = renderer.render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 1.,
            ..Default::default()
        });
        let mut pixels = image.rgba.to_vec();
        for (i, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let paper = 225. + 25. * (i % image.width as usize) as f32 / image.width as f32;
            let lum = pixel[0] as f32;
            pixel[..3].fill((paper - (255. - lum) * 0.045).round() as u8);
        }
        let baseline = windows_recognize(&pixels, image.width, image.height).unwrap();
        let result = crate::ocr::recognize(&pixels, image.width, image.height).unwrap();
        assert!(
            result.text().contains("Été français 2026"),
            "baseline: {}, enhanced: {}",
            baseline.text(),
            result.text()
        );
        assert_eq!(result.text().matches("2026").count(), 1);
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-output/ocr-faint-input.png"),
            crate::ocr::encode_png(&pixels, image.width, image.height).unwrap(),
        )
        .unwrap();
        println!(
            "Faint scan: before={:?}; after={:?}",
            baseline.text(),
            result.text()
        );
    }
}
