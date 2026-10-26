//! Development-only recognition regression on an explicitly supplied PDF.
use pdfinator::{
    document::Editor,
    ocr::{self, ImageSource},
};
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("PDF absent")?;
    let e = Editor::open(std::path::Path::new(&path))?;
    let (bytes, area) = e.image_for_ocr(0, ImageSource::Original(0))?;
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page: 0,
        scale: 3.,
        tile: Some(pdfcraft_render::Tile {
            x: (area[0] * 3.).floor() as u32,
            y: (area[1] * 3.).floor() as u32,
            w: ((area[2] * 3.).ceil() - (area[0] * 3.).floor()) as u32,
            h: ((area[3] * 3.).ceil() - (area[1] * 3.).floor()) as u32,
        }),
        ..Default::default()
    });
    if let Some(e) = image.error {
        return Err(e);
    }
    std::fs::write(
        "test-output/diagram-ocr-input.png",
        ocr::encode_png(&image.rgba, image.width, image.height)?,
    )
    .map_err(|e| e.to_string())?;
    let result = ocr::recognize(&image.rgba, image.width, image.height)?;
    for line in result.lines {
        println!("{:?}: {}", line.rect, line.text);
    }
    Ok(())
}
