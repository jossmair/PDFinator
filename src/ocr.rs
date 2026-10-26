//! Local image recognition. No process, service, download or network access.
use crate::document::Result;
#[cfg(any(windows, test))]
mod enhancement;
mod regions;
pub use regions::{Block, encode_png, erase_letters};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageSource {
    Original(usize),
    Added(usize),
}

#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    /// Image pixels, with a top-left origin.
    pub rect: [f32; 4],
    pub words: Vec<Word>,
}
#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Recognition {
    pub language: String,
    pub lines: Vec<Line>,
}
impl Recognition {
    pub fn blocks(&self) -> Vec<Block> {
        regions::blocks(&self.lines)
    }
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<Recognition> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if width == 0 || height == 0 || width > 4096 || height > 4096 || expected != Some(rgba.len()) {
        return Err("Dimensions d’image non prises en charge pour la reconnaissance.".into());
    }
    #[cfg(windows)]
    {
        let mut result = windows_recognize(rgba, width, height)?;
        let (prepared, pw, ph) = enhancement::prepare(rgba, width, height, &result);
        if let Ok(second) = windows_recognize(&prepared, pw, ph) {
            enhancement::merge(
                &mut result,
                second,
                width as f32 / pw as f32,
                height as f32 / ph as f32,
            );
        }
        enhancement::repair_degrees(rgba, width, height, &mut result);
        recover_labels(rgba, width, height, &mut result);
        for line in &mut result.lines {
            line.words
                .retain(|word| !attached_arrow(rgba, width, height, word));
            if !line.words.is_empty() {
                line.text = line
                    .words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        result.lines.retain(|line| !line.words.is_empty());
        if result.lines.is_empty() {
            return Err("Aucun texte lisible n’a été reconnu dans cette image. Essayez une image plus nette.".into());
        }
        Ok(result)
    }
    #[cfg(not(windows))]
    Err("La reconnaissance locale nécessite Windows 10 ou 11.".into())
}

/// A downward arrowhead is sometimes read as Y: its upper half is a solid
/// triangle, unlike the two separate arms of Y. Some also have an attached stem.
#[cfg(any(windows, test))]
fn attached_arrow(rgba: &[u8], width: u32, height: u32, word: &Word) -> bool {
    if word.text != "Y" || word.rect.iter().any(|v| !v.is_finite()) {
        return false;
    }
    let h = (word.rect[3] - word.rect[1]).ceil().max(1.) as u32;
    let top = word.rect[1].floor().max(0.) as u32;
    let dark = |x, y| {
        let i = ((y * width + x) * 4) as usize;
        rgba[i..i + 3].iter().map(|c| *c as u32).sum::<u32>() < 480
    };
    let left = word.rect[0].floor().max(0.).min(width as f32) as u32;
    let right = word.rect[2].ceil().max(0.).min(width as f32) as u32;
    let bottom = word.rect[3].ceil().max(0.).min(height as f32) as u32;
    let rows: Vec<_> = (top.min(height)..bottom)
        .filter_map(|y| {
            let xs: Vec<_> = (left..right).filter(|x| dark(*x, y)).collect();
            let span = xs.last()? - xs.first()? + 1;
            Some((span, xs.len() as f32 / span as f32))
        })
        .collect();
    if rows.len() >= 4 {
        let n = rows.len() / 2;
        let solid = rows[..n].iter().all(|(_, fill)| *fill >= 0.85);
        let wide = rows[..n].iter().map(|(w, _)| *w).max().unwrap_or(0);
        let tip = rows[n..].iter().map(|(w, _)| *w).min().unwrap_or(0);
        if solid && wide as f32 >= rows.len() as f32 * 0.4 && tip * 2 < wide {
            return true;
        }
    }
    if top < h + 2 || top >= height || h < 4 {
        return false;
    }
    let middle = (word.rect[0] + word.rect[2]) * 0.5;
    let left = (middle - 1.).max(0.) as u32;
    let right = (middle + 2.).max(0.).min(width as f32) as u32;
    let stem = |y| (left..right).any(|x| dark(x, y));
    stem(top - 1)
        && stem(top - 2)
        && (top - h - 2..top - 2).filter(|y| stem(*y)).count() * 5 >= h as usize * 4
}

/// Diagram labels are often discarded by the page recognizer because a single
/// letter is not a line of prose. Retry only isolated ink components, at a readable
/// size, without inventing labels or changing their original position.
#[cfg(windows)]
fn recover_labels(rgba: &[u8], width: u32, height: u32, result: &mut Recognition) {
    let w = width as usize;
    let h = height as usize;
    let mut seen = vec![false; w * h];
    for line in &result.lines {
        for r in line
            .words
            .iter()
            .map(|v| v.rect)
            .chain(std::iter::once(line.rect))
        {
            for y in (r[1] - 2.).max(0.) as usize..(r[3] + 2.).min(height as f32) as usize {
                for x in (r[0] - 2.).max(0.) as usize..(r[2] + 2.).min(width as f32) as usize {
                    seen[y * w + x] = true;
                }
            }
        }
    }
    let ink = |i: usize| {
        rgba[i * 4..i * 4 + 3]
            .iter()
            .map(|c| *c as u32)
            .sum::<u32>()
            < 480
    };
    let mut attempts = 0;
    for at in 0..w * h {
        if seen[at] || !ink(at) {
            continue;
        }
        seen[at] = true;
        let mut component = vec![at];
        let mut bounds = [at % w, at / w, at % w + 1, at / w + 1];
        let mut cursor = 0;
        while cursor < component.len() {
            let p = component[cursor];
            cursor += 1;
            let (x, y) = (p % w, p / w);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x + 1),
                bounds[3].max(y + 1),
            ];
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let q = ny * w + nx;
                    if !seen[q] && ink(q) {
                        seen[q] = true;
                        component.push(q);
                    }
                }
            }
        }
        let cw = bounds[2] - bounds[0];
        let ch = bounds[3] - bounds[1];
        if !(6..=128).contains(&ch)
            || cw < ch / 5
            || cw > ch * 2
            || component.len() < ch * 2
            || attempts >= 64
        {
            continue;
        }
        attempts += 1;
        let scale = 48. / ch as f32;
        let glyph_width = (cw as f32 * scale).ceil() as u32;
        let Some((anchor_width, anchor)) = label_context() else {
            return;
        };
        let anchor_width = *anchor_width;
        let ow = glyph_width + anchor_width + 96;
        let oh = 112;
        let mut crop = vec![255; ow as usize * oh as usize * 4];
        for y in 0..48 {
            let dst = ((y + 32) * ow + 32) as usize * 4;
            let src = (y * anchor_width) as usize * 4;
            crop[dst..dst + anchor_width as usize * 4]
                .copy_from_slice(&anchor[src..src + anchor_width as usize * 4]);
        }
        // Preserve the component's anti-aliasing and exclude adjacent construction lines.
        for dy in 0..48 {
            for dx in 0..glyph_width {
                let x = bounds[0] + (dx as f32 / scale) as usize;
                let y = bounds[1] + (dy as f32 / scale) as usize;
                if x < bounds[2] && y < bounds[3] {
                    let src = (y * w + x) * 4;
                    let dst = ((dy + 32) * ow + dx + anchor_width + 56) as usize * 4;
                    crop[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
        }
        if let Ok(recognized) = windows_recognize(&crop, ow, oh) {
            let letters = recognized
                .lines
                .iter()
                .flat_map(|l| &l.words)
                .filter(|word| word.rect[0] > anchor_width as f32 + 40.)
                .flat_map(|word| word.text.chars())
                .collect::<Vec<_>>();
            let Some(&letter) = letters.first() else {
                continue;
            };
            if !letter.is_alphabetic() || letters.len() != 1 {
                continue;
            }
            let text = letter.to_string();
            let rect = bounds.map(|v| v as f32);
            result.lines.push(Line {
                text: text.clone(),
                rect,
                words: vec![Word { text, rect }],
            });
        }
    }
    result.lines.sort_by(|a, b| {
        a.rect[1]
            .total_cmp(&b.rect[1])
            .then_with(|| a.rect[0].total_cmp(&b.rect[0]))
    });
}

/// A fixed, rendered context word helps Windows accept isolated diagram letters.
/// It is never part of the recognized document or of the saved PDF.
#[cfg(windows)]
fn label_context() -> Option<&'static (u32, Vec<u8>)> {
    static CONTEXT: std::sync::OnceLock<Option<(u32, Vec<u8>)>> = std::sync::OnceLock::new();
    CONTEXT
        .get_or_init(|| {
            let doc = pdfcraft_create::from_text("", "Point", (300., 160.), 48.).ok()?;
            let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).ok()?;
            let mut renderer =
                pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes), Default::default());
            let image = renderer.render(pdfcraft_render::RenderRequest {
                page: 0,
                scale: 1.,
                ..Default::default()
            });
            if image.error.is_some() {
                return None;
            }
            let mut b = [image.width, image.height, 0, 0];
            for y in 0..image.height {
                for x in 0..image.width {
                    let i = ((y * image.width + x) * 4) as usize;
                    if image.rgba[i..i + 3].iter().any(|c| *c < 160) {
                        b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
                    }
                }
            }
            if b[3] <= b[1] {
                return None;
            }
            let s = 48. / (b[3] - b[1]) as f32;
            let w = ((b[2] - b[0]) as f32 * s).ceil() as u32;
            let mut rgba = vec![255; w as usize * 48 * 4];
            for y in 0..48 {
                for x in 0..w {
                    let sx = (b[0] + (x as f32 / s) as u32).min(b[2] - 1);
                    let sy = (b[1] + (y as f32 / s) as u32).min(b[3] - 1);
                    let src = ((sy * image.width + sx) * 4) as usize;
                    let dst = ((y * w + x) * 4) as usize;
                    rgba[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
                }
            }
            Some((w, rgba))
        })
        .as_ref()
}

#[cfg(windows)]
fn windows_recognize(rgba: &[u8], width: u32, height: u32) -> Result<Recognition> {
    use windows::{
        Globalization::Language,
        Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
    };
    fn run(rgba: &[u8], width: u32, height: u32) -> windows::core::Result<Recognition> {
        let languages = OcrEngine::AvailableRecognizerLanguages()?;
        let french = Language::CreateLanguage(&windows::core::HSTRING::from("fr-FR"))?;
        let engine = if OcrEngine::IsLanguageSupported(&french)? {
            OcrEngine::TryCreateFromLanguage(&french)?
        } else {
            OcrEngine::TryCreateFromUserProfileLanguages()
                .or_else(|_| OcrEngine::TryCreateFromLanguage(&languages.GetAt(0)?))?
        };
        if width > OcrEngine::MaxImageDimension()? || height > OcrEngine::MaxImageDimension()? {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80070057_u32 as i32),
                "Image trop grande",
            ));
        }
        let mut bgra = rgba.to_vec();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &writer.DetachBuffer()?,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
            BitmapAlphaMode::Ignore,
        )?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let language = engine.RecognizerLanguage()?.LanguageTag()?.to_string();
        let mut lines = Vec::new();
        for line in result.Lines()?.into_iter().take(4096) {
            let text = line.Text()?.to_string();
            if text.trim().is_empty() {
                continue;
            }
            let mut rect = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            let mut words = Vec::new();
            for word in line.Words()? {
                let b = word.BoundingRect()?;
                words.push(Word {
                    text: word.Text()?.to_string(),
                    rect: [b.X, b.Y, b.X + b.Width, b.Y + b.Height],
                });
                rect = [
                    rect[0].min(b.X),
                    rect[1].min(b.Y),
                    rect[2].max(b.X + b.Width),
                    rect[3].max(b.Y + b.Height),
                ];
            }
            if rect.iter().all(|v| v.is_finite()) && rect[2] > rect[0] && rect[3] > rect[1] {
                lines.push(Line { text, rect, words });
            }
        }
        bitmap.Close()?;
        Ok(Recognition { language, lines })
    }
    let result = run(rgba, width, height).map_err(|_| String::from(
        "La reconnaissance Windows n’est pas disponible. Vérifiez qu’une langue de reconnaissance est installée dans les paramètres Windows. Aucun changement appliqué.",
    ))?;
    Ok(result)
}

#[cfg(test)]
mod arrow_tests {
    use super::*;
    #[test]
    fn an_arrow_stem_is_kept_as_graphics_while_a_real_y_remains_editable() {
        let mut rgba = vec![255; 100 * 100 * 4];
        let word = Word {
            text: "Y".into(),
            rect: [40., 60., 60., 80.],
        };
        assert!(!attached_arrow(&rgba, 100, 100, &word));
        for y in 25..60 {
            let at = (y * 100 + 50) * 4;
            rgba[at..at + 3].fill(0);
        }
        assert!(attached_arrow(&rgba, 100, 100, &word));
        rgba.fill(255);
        for y in 60..80 {
            let half = 10 - (y - 60) / 2;
            for x in 50 - half..50 + half {
                let at = (y * 100 + x) * 4;
                rgba[at..at + 3].fill(0);
            }
        }
        assert!(
            attached_arrow(&rgba, 100, 100, &word),
            "The triangular tip remains graphics even with a pale stem"
        );
        rgba.fill(255);
        for y in 60..80 {
            let xs = if y < 70 {
                vec![40 + (y - 60), 59 - (y - 60)]
            } else {
                vec![50]
            };
            for x in xs {
                let at = (y * 100 + x) * 4;
                rgba[at..at + 3].fill(0);
            }
        }
        assert!(
            !attached_arrow(&rgba, 100, 100, &word),
            "The two separate arms identify a real Y label"
        );
        let a = Word {
            text: "A".into(),
            ..word
        };
        assert!(!attached_arrow(&rgba, 100, 100, &a));
    }
}
