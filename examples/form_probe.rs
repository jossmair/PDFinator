//! Read-only inventory for explicitly supplied group-editing regression PDFs.
fn main() -> Result<(), String> {
    for path in std::env::args_os().skip(1) {
        let e = pdfinator::document::Editor::open(std::path::Path::new(&path))?;
        for page in 0..e.info.pages.len() {
            let images = pdfcraft_edit::page_images(&e.doc, page).map_err(|e| e.to_string())?;
            for (i, image) in images.iter().enumerate().filter(|(_, i)| i.is_form) {
                let o = image.object.map(|r| e.doc.get(r));
                let d = o.as_ref().and_then(|o| o.as_dict());
                let keys = d
                    .and_then(|d| d.get(b"Resources"))
                    .and_then(|r| e.doc.dict(r))
                    .map(|r| {
                        r.iter()
                            .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                            .collect::<Vec<_>>()
                    });
                println!(
                    "{} page {} form {} editable={} resources={keys:?} matrix={:?} transparency={}",
                    std::path::Path::new(&path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    page + 1,
                    i,
                    e.can_dissociate_form(page, i),
                    image.matrix,
                    d.is_some_and(|d| d.get(b"Group").is_some())
                );
            }
        }
    }
    Ok(())
}
