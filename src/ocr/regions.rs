use super::{Line, Word};
use crate::document::Result;

#[derive(Clone, Debug)]
pub struct Block {
    pub text: String,
    /// Initially pixels; the desktop converts this to display points once.
    pub rect: [f32; 4],
    pub size: f64,
    pub color: [f64; 3],
    pub words: Vec<[f32; 4]>,
}
pub(super) fn blocks(lines: &[Line]) -> Vec<Block> {
    let mut result: Vec<Block> = Vec::new();
    for line in lines {
        let words = if line.words.is_empty() {
            vec![Word {
                text: line.text.clone(),
                rect: line.rect,
            }]
        } else {
            line.words.clone()
        };
        let mut current: Option<Block> = None;
        for word in words {
            let height = (word.rect[3] - word.rect[1]).max(1.);
            let join = current.as_ref().is_some_and(|b| {
                word.rect[0] - b.rect[2] < height * 1.7
                    && ((word.rect[1] + word.rect[3] - b.rect[1] - b.rect[3]) / 2.).abs()
                        < height * 0.6
            });
            if !join && let Some(b) = current.take() {
                result.push(b);
            }
            if let Some(b) = &mut current {
                b.text.push(' ');
                b.text.push_str(&word.text);
                b.rect = [
                    b.rect[0].min(word.rect[0]),
                    b.rect[1].min(word.rect[1]),
                    b.rect[2].max(word.rect[2]),
                    b.rect[3].max(word.rect[3]),
                ];
                b.words.push(word.rect);
            } else {
                current = Some(Block {
                    text: word.text,
                    rect: word.rect,
                    size: (height * 1.15) as f64,
                    color: [0.; 3],
                    words: vec![word.rect],
                });
            }
        }
        if let Some(b) = current {
            result.push(b);
        }
    }
    result
}
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(rgba)
            .map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}
/// Change the raster itself, not a white PDF overlay. Only recognised word boxes
/// are reconstructed; all pixels elsewhere stay byte-for-byte unchanged.
pub fn erase_letters(
    rgba: &[u8],
    width: u32,
    height: u32,
    blocks: &mut [Block],
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err("Image de reconnaissance invalide.".into());
    }
    let mut result = rgba.to_vec();
    let offset = |x: u32, y: u32| ((y * width + x) * 4) as usize;
    for block in blocks {
        let mut darkest = [255_u8; 3];
        for r in &block.words {
            let x0 = (r[0].floor() - 1.).max(0.) as u32;
            let y0 = (r[1].floor() - 1.).max(0.) as u32;
            let x1 = (r[2].ceil() + 1.).min(width as f32) as u32;
            let y1 = (r[3].ceil() + 1.).min(height as f32) as u32;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let mut samples = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for y in y0.saturating_sub(3)..(y1 + 3).min(height) {
                for x in x0.saturating_sub(3)..(x1 + 3).min(width) {
                    if x >= x0 && x < x1 && y >= y0 && y < y1 {
                        continue;
                    }
                    let i = offset(x, y);
                    for c in 0..4 {
                        samples[c].push(rgba[i + c]);
                    }
                }
            }
            let mut background = [255_u8; 4];
            for c in 0..4 {
                if !samples[c].is_empty() {
                    samples[c].sort_unstable();
                    background[c] = samples[c][samples[c].len() / 2];
                }
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = offset(x, y);
                    let p = &rgba[i..i + 3];
                    if p.iter().map(|v| *v as u32).sum::<u32>()
                        < darkest.iter().map(|v| *v as u32).sum()
                    {
                        darkest.copy_from_slice(p);
                    }
                    result[i..i + 4].copy_from_slice(&background);
                }
            }
            // A ruled table may cross a label. Retain horizontal/vertical strokes
            // continuing beyond both sides of its word box.
            let dark = |x, y| {
                let i = offset(x, y);
                (0..3)
                    .map(|c| (rgba[i + c] as i32 - background[c] as i32).unsigned_abs())
                    .sum::<u32>()
                    > 120
            };
            for y in y0..y1 {
                if x0 >= 4 && x1 + 4 < width && (1..=4).all(|d| dark(x0 - d, y) && dark(x1 + d, y))
                {
                    for x in x0..x1 {
                        let i = offset(x, y);
                        result[i..i + 4].copy_from_slice(&rgba[i..i + 4]);
                    }
                }
            }
            for x in x0..x1 {
                if y0 >= 4 && y1 + 4 < height && (1..=4).all(|d| dark(x, y0 - d) && dark(x, y1 + d))
                {
                    for y in y0..y1 {
                        let i = offset(x, y);
                        result[i..i + 4].copy_from_slice(&rgba[i..i + 4]);
                    }
                }
            }
        }
        block.color = darkest.map(|v| v as f64 / 255.);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_labels_keep_their_positions_and_the_diagram_remains_in_the_background() {
        let line = Line {
            text: "A 5 cm B".into(),
            rect: [10., 10., 190., 30.],
            words: vec![
                Word {
                    text: "A".into(),
                    rect: [10., 10., 20., 30.],
                },
                Word {
                    text: "5".into(),
                    rect: [60., 10., 70., 30.],
                },
                Word {
                    text: "cm".into(),
                    rect: [75., 10., 95., 30.],
                },
                Word {
                    text: "B".into(),
                    rect: [180., 10., 190., 30.],
                },
            ],
        };
        let mut blocks = blocks(&[line]);
        assert_eq!(
            blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
            vec!["A", "5 cm", "B"]
        );
        let mut pixels = vec![255_u8; 200 * 100 * 4];
        for x in 20..180 {
            let i = (60 * 200 + x) * 4;
            pixels[i..i + 3].fill(0);
        }
        for y in 12..28 {
            let i = (y * 200 + 15) * 4;
            pixels[i..i + 3].fill(0);
        }
        let clean = erase_letters(&pixels, 200, 100, &mut blocks).unwrap();
        assert_eq!(
            &clean[(60 * 200 + 80) * 4..(60 * 200 + 80) * 4 + 3],
            &[0, 0, 0]
        );
        assert_eq!(
            &clean[(20 * 200 + 15) * 4..(20 * 200 + 15) * 4 + 3],
            &[255, 255, 255]
        );
        for y in 31..100 {
            assert_eq!(
                &clean[y * 200 * 4..(y + 1) * 200 * 4],
                &pixels[y * 200 * 4..(y + 1) * 200 * 4]
            );
        }
    }
}
