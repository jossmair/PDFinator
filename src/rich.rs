//! Character formatting in UTF-8 ranges, independent of the user interface.
use serde::{Deserialize, Serialize};
use std::ops::Range;
mod paragraphs;
pub use paragraphs::{bulleted, toggle_bullets};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Format {
    pub resource: Option<String>,
    pub font_name: Option<String>,
    pub family: Option<String>,
    pub size: Option<f64>,
    pub color: Option<[f64; 3]>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub highlight: Option<bool>,
    pub background: Option<[f64; 3]>,
    pub script: Option<i8>,
}
impl Format {
    pub fn overlay(&mut self, other: &Self) {
        macro_rules! set{($($field:ident),*)=>{$(if other.$field.is_some(){self.$field=other.$field.clone();})*}}
        set!(
            resource, font_name, family, size, color, bold, italic, underline, strike, highlight,
            background, script
        );
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub range: Range<usize>,
    pub format: Format,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextData {
    pub id: String,
    pub text: String,
    pub rect: [f64; 4],
    pub base_size: f64,
    pub base_color: [f64; 3],
    pub base_bold: bool,
    pub base_italic: bool,
    pub font_name: String,
    pub align: Option<String>,
    #[serde(default)]
    pub line_spacing: Option<f64>,
    pub spans: Vec<Span>,
}
impl TextData {
    /// Private editor data is still untrusted when it comes from an opened PDF.
    pub fn valid(&self) -> bool {
        let color = |c: [f64; 3]| c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v));
        self.text.len() <= 4_000_000
            && self.spans.len() <= 1024
            && self
                .rect
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 10_000_000.)
            && self.rect[2] > self.rect[0]
            && self.rect[3] > self.rect[1]
            && self.base_size.is_finite()
            && (0.1..=14400.).contains(&self.base_size)
            && color(self.base_color)
            && self
                .line_spacing
                .is_none_or(|v| v.is_finite() && (0.5..=5.0).contains(&v))
            && self.spans.iter().all(|s| {
                let f = &s.format;
                !s.range.is_empty()
                    && self.text.get(s.range.clone()).is_some()
                    && f.size
                        .is_none_or(|v| v.is_finite() && (0.1..=14400.).contains(&v))
                    && f.color.is_none_or(color)
                    && f.background.is_none_or(color)
                    && f.script.is_none_or(|v| (-1..=1).contains(&v))
            })
            && self
                .spans
                .windows(2)
                .all(|s| s[0].range.end <= s[1].range.start)
    }
}
pub fn apply(spans: &mut Vec<Span>, range: Range<usize>, delta: &Format) {
    if range.is_empty() {
        return;
    }
    let mut edges = vec![range.start, range.end];
    for span in spans.iter() {
        edges.extend([span.range.start, span.range.end]);
    }
    edges.sort_unstable();
    edges.dedup();
    let mut output: Vec<Span> = Vec::new();
    for edge in edges.windows(2) {
        let start = edge[0];
        let end = edge[1];
        let mut f = spans
            .iter()
            .find(|s| s.range.contains(&start))
            .map(|s| s.format.clone())
            .unwrap_or_default();
        if range.contains(&start) {
            f.overlay(delta);
        }
        if f == Format::default() {
            continue;
        }
        if let Some(previous) = output
            .last_mut()
            .filter(|p| p.range.end == start && p.format == f)
        {
            previous.range.end = end;
        } else {
            output.push(Span {
                range: start..end,
                format: f,
            });
        }
    }
    *spans = output;
}
pub fn remap(spans: &mut Vec<Span>, old: &str, new: &str) {
    if old == new {
        return;
    }
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let mut output = Vec::new();
    let inherited = spans
        .iter()
        .find(|s| s.range.contains(&prefix) || s.range.end == prefix)
        .map(|s| s.format.clone());
    for span in spans.iter() {
        if span.range.start < prefix {
            let stop = span.range.end.min(prefix);
            if stop > span.range.start {
                output.push(Span {
                    range: span.range.start..stop,
                    format: span.format.clone(),
                });
            }
        }
        if span.range.end > end {
            let start = span.range.start.max(end) - end + new_end;
            let stop = span.range.end - end + new_end;
            if stop > start {
                output.push(Span {
                    range: start..stop,
                    format: span.format.clone(),
                });
            }
        }
    }
    if new_end > prefix
        && let Some(format) = inherited
    {
        output.push(Span {
            range: prefix..new_end,
            format,
        });
    }
    output.sort_by_key(|s| s.range.start);
    *spans = output;
}
pub fn byte_range(text: &str, chars: Range<usize>) -> Option<Range<usize>> {
    let index = |n| {
        text.char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .nth(n)
    };
    Some(index(chars.start)?..index(chars.end)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatting_tracks_unicode_replacements_without_cutting_characters() {
        let old = "Un été à Paris.";
        let start = old.find("été").unwrap();
        let mut spans = Vec::new();
        apply(
            &mut spans,
            start..start + "été à".len(),
            &Format {
                color: Some([1., 0., 0.]),
                ..Default::default()
            },
        );
        let new = "Un été très agréable à Paris.";
        remap(&mut spans, old, new);
        assert!(spans.iter().all(|s| new.get(s.range.clone()).is_some()));
        let styled: String = spans.iter().map(|s| &new[s.range.clone()]).collect();
        assert_eq!(styled, "été très agréable à");
        assert_eq!(byte_range("éœa", 1..2), Some(2..4));
        assert_eq!(byte_range("éœa", 4..5), None);
    }
}
