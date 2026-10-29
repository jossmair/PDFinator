//! Transactional operations on a non-empty set of pages, in document order.
use crate::document::{Editor, Result, guarded};

fn indices(pages: &[usize], count: usize) -> Result<Vec<usize>> {
    if pages.is_empty() || pages.iter().any(|&page| page >= count) {
        return Err("Sélection de pages incorrecte.".into());
    }
    let mut pages = pages.to_vec();
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

impl Editor {
    /// `destination` is the start index of the group in the resulting document.
    pub fn move_page_group(&mut self, pages: &[usize], destination: usize) -> Result<()> {
        let count = self.info.pages.len();
        let pages = indices(pages, count)?;
        if destination > count - pages.len() {
            return Err("Cette position dépasse la fin du document.".into());
        }
        let mut order: Vec<_> = (0..count).filter(|p| !pages.contains(p)).collect();
        order.splice(destination..destination, pages.iter().copied());
        if order.iter().copied().eq(0..count) {
            return Ok(());
        }
        self.change(false, |doc| {
            pdfcraft_organize::move_pages(doc, &pages, destination).map_err(|e| e.to_string())
        })
    }

    /// Copies are inserted together, immediately after the last selected page.
    pub fn duplicate_page_group(&mut self, pages: &[usize]) -> Result<std::ops::Range<usize>> {
        let pages = indices(pages, self.info.pages.len())?;
        let at = pages[pages.len() - 1] + 1;
        self.change_pages(false, self.info.pages.len() + pages.len(), |doc| {
            pdfcraft_organize::duplicate_pages(doc, &pages, Some(at)).map_err(|e| e.to_string())
        })?;
        Ok(at..at + pages.len())
    }

    pub fn delete_page_group(&mut self, pages: &[usize]) -> Result<()> {
        let pages = indices(pages, self.info.pages.len())?;
        if pages.len() == self.info.pages.len() {
            return Err("Le document doit conserver au moins une page.".into());
        }
        self.change_pages(false, self.info.pages.len() - pages.len(), |doc| {
            pdfcraft_organize::delete_pages(doc, &pages).map_err(|e| e.to_string())
        })
    }

    pub fn rotate_page_group(&mut self, pages: &[usize], degrees: i64) -> Result<()> {
        let pages = indices(pages, self.info.pages.len())?;
        if degrees % 90 != 0 {
            return Err("La rotation doit être un multiple de 90°.".into());
        }
        if degrees % 360 == 0 {
            return Ok(());
        }
        let degrees = degrees.rem_euclid(360);
        self.change(false, |doc| {
            pdfcraft_organize::rotate_pages(doc, &pages, degrees).map_err(|e| e.to_string())?;
            let added: Vec<_> = pdfcraft_edit::list_added(doc)
                .into_iter()
                .filter(|a| pages.contains(&a.page))
                .map(|a| a.obj)
                .collect();
            for object in added {
                doc.update_dict(object, |dict| {
                    dict.remove(b"PCMark");
                    dict.remove(b"PCAdded");
                })
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    /// Produces a new unsaved PDF; never changes the source or its history.
    pub fn extract_page_group(&self, pages: &[usize]) -> Result<Vec<u8>> {
        if !self.can_copy() {
            return Err("Les permissions de ce PDF interdisent l’extraction.".into());
        }
        let pages = indices(pages, self.info.pages.len())?;
        guarded(|| {
            let doc =
                pdfcraft_organize::extract_pages(&self.doc, &pages).map_err(|e| e.to_string())?;
            pdfcraft_cos::write_full(&doc, &Default::default()).map_err(|e| e.to_string())
        })
    }
}
