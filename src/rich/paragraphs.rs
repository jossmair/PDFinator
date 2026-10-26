use super::{Span, remap};
use std::ops::Range;

const BULLET: &str = "• ";

fn starts(text: &str, range: Option<Range<usize>>) -> Vec<usize> {
    let range = range.unwrap_or(0..text.len());
    if text.get(range.clone()).is_none() {
        return Vec::new();
    }
    let end = range.end.saturating_sub(usize::from(!range.is_empty()));
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .filter(|&start| {
            let stop = text[start..].find('\n').map_or(text.len(), |i| start + i);
            start <= end && stop >= range.start
        })
        .map(|start| {
            start
                + text[start..]
                    .chars()
                    .take_while(|c| matches!(c, ' ' | '\t'))
                    .map(char::len_utf8)
                    .sum::<usize>()
        })
        .collect()
}

pub fn bulleted(text: &str, range: Option<Range<usize>>) -> bool {
    let lines = starts(text, range);
    !lines.is_empty() && lines.iter().all(|&i| text[i..].starts_with(BULLET))
}

/// Toggle the touched paragraphs. Edits run backwards so independent rich
/// ranges stay attached to their letters; positions are UTF-8 byte offsets.
pub fn toggle_bullets(
    text: &mut String,
    spans: &mut Vec<Span>,
    range: Option<Range<usize>>,
    positions: &mut [usize],
) {
    let lines = starts(text, range);
    let remove = !lines.is_empty() && lines.iter().all(|&i| text[i..].starts_with(BULLET));
    for start in lines.into_iter().rev() {
        if !remove && text[start..].starts_with(BULLET) {
            continue;
        }
        let count = if remove { BULLET.len() } else { 0 };
        let prefix = if remove { "" } else { BULLET };
        let mut new = text.clone();
        new.replace_range(start..start + count, prefix);
        remap(spans, text, &new);
        *text = new;
        for position in positions.iter_mut() {
            if *position >= start {
                *position = start + position.saturating_sub(start + count) + prefix.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich::{Format, apply};

    #[test]
    fn bullets_preserve_unicode_styles_selection_and_indented_paragraphs() {
        let mut text = "Été\n  déjà là\nFin".to_string();
        let original = text.clone();
        let mut spans = Vec::new();
        let start = text.find("déjà").unwrap();
        apply(
            &mut spans,
            start..start + "déjà".len(),
            &Format {
                bold: Some(true),
                ..Default::default()
            },
        );
        let mut caret = [start + "déjà".len()];
        let range = Some(start..caret[0]);
        toggle_bullets(&mut text, &mut spans, range, &mut caret);
        assert_eq!(text, "Été\n  • déjà là\nFin");
        assert!(
            spans
                .iter()
                .any(|s| &text[s.range.clone()] == "déjà" && s.format.bold == Some(true))
        );
        assert_eq!(&text[..caret[0]], "Été\n  • déjà");
        assert!(bulleted(&text, Some(caret[0]..caret[0])));
        toggle_bullets(&mut text, &mut spans, Some(caret[0]..caret[0]), &mut caret);
        assert_eq!(text, original);
        assert_eq!(caret[0], start + "déjà".len());
    }

    #[test]
    fn mixed_lists_toggle_without_duplicate_markers_or_touching_next_paragraph() {
        let mut text = "• Un\nDeux\nTrois".to_string();
        let end = text.find("Trois").unwrap();
        toggle_bullets(&mut text, &mut vec![], Some(0..end), &mut []);
        assert_eq!(text, "• Un\n• Deux\nTrois");
        toggle_bullets(&mut text, &mut vec![], None, &mut []);
        assert_eq!(text, "• Un\n• Deux\n• Trois");
        toggle_bullets(&mut text, &mut vec![], None, &mut []);
        assert_eq!(text, "Un\nDeux\nTrois");
    }
}
