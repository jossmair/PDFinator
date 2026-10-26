//! Page ranges are validated before any spooler operation.
use crate::document::{Editor, Result, guarded};
pub fn parse(input: &str, count: usize) -> Result<Vec<usize>> {
    let mut pages = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let number = |s: &str| -> Result<usize> {
        let n = s
            .trim()
            .parse::<usize>()
            .map_err(|_| "Indiquez des pages comme 1-3, 5, 8.".to_string())?;
        if n == 0 || n > count {
            return Err(format!(
                "Les pages doivent être comprises entre 1 et {count}."
            ));
        }
        Ok(n - 1)
    };
    if input.trim().is_empty() {
        return Err("Indiquez les pages à imprimer.".into());
    }
    for part in input.split(',') {
        let part = part.replace(['–', '—'], "-");
        let (start, end) = if let Some((a, b)) = part.split_once('-') {
            (number(a)?, number(b)?)
        } else {
            let n = number(&part)?;
            (n, n)
        };
        if end < start {
            return Err("Le début d’une plage doit précéder sa fin.".into());
        }
        for page in start..=end {
            if seen.insert(page) {
                pages.push(page);
            }
        }
    }
    Ok(pages)
}
pub fn subset(bytes: std::sync::Arc<Vec<u8>>, pages: &[usize]) -> Result<Vec<u8>> {
    guarded(|| {
        let e = Editor::from_bytes(bytes.as_ref().clone())?;
        if e.doc.permissions().is_some_and(|p| !p.print()) {
            return Err("Les permissions de ce PDF interdisent l’impression.".into());
        }
        if pages.is_empty() || pages.iter().any(|&p| p >= e.info.pages.len()) {
            return Err("Plage de pages incorrecte.".into());
        }
        let doc = pdfcraft_organize::extract_pages(&e.doc, pages).map_err(|e| e.to_string())?;
        pdfcraft_cos::write_full(&doc, &Default::default()).map_err(|e| e.to_string())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_ranges_reject_invalid_bounds_and_keep_requested_order_without_duplicates() {
        assert_eq!(parse("3, 1-2, 3, 5–6", 6).unwrap(), vec![2, 0, 1, 4, 5]);
        for bad in ["", "0", "7", "3-1", "1-2-3", "1,", "-1"] {
            assert!(parse(bad, 6).is_err(), "{bad}");
        }
        assert!(parse("1", 0).is_err());
    }
    #[test]
    fn printing_subset_keeps_page_order_and_leaves_the_source_unchanged() {
        let doc = pdfcraft_create::blank(420., 595., 3).unwrap();
        let bytes =
            std::sync::Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
        let mut source = Editor::from_bytes(bytes.as_ref().clone()).unwrap();
        source.rotate_page(0, 90).unwrap();
        source.add_comment(2, [20., 50.], "Note conservée").unwrap();
        let bytes = source.bytes.clone();
        let before = bytes.clone();
        let out = subset(bytes.clone(), &[2, 0]).unwrap();
        let e = Editor::from_bytes(out).unwrap();
        assert_eq!(e.info.pages.len(), 2);
        assert!((e.info.pages[0].width - 420.).abs() < 0.1);
        assert!((e.info.pages[1].width - 595.).abs() < 0.1);
        assert!(
            e.info
                .annotations
                .iter()
                .any(|a| a.page == 0 && a.contents.as_deref() == Some("Note conservée"))
        );
        assert_eq!(bytes, before);
    }
}
