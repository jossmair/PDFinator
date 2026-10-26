//! Development-only real Windows OCR check, without accessing personal files.
fn main() -> Result<(), String> {
    let doc = pdfcraft_create::from_text(
        "OCR",
        "Bonjour Ergonatus 123\nÉcole française : été, à bientôt !",
        (595., 842.),
        22.,
    )
    .map_err(|e| e.to_string())?;
    let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).map_err(|e| e.to_string())?;
    let mut renderer =
        pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes), Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page: 0,
        scale: 2.,
        ..Default::default()
    });
    if let Some(error) = image.error {
        return Err(error);
    }
    let result = pdfinator::ocr::recognize(&image.rgba, image.width, image.height)?;
    println!("Langue : {}\n{}", result.language, result.text());
    if !result.text().contains("Ergonatus 123") {
        return Err("La reconnaissance du texte témoin a échoué.".into());
    }
    Ok(())
}
