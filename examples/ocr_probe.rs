//! Development-only Windows OCR check; an explicit PNG path enables diagram diagnostics.
fn main() -> Result<(), String> {
    if let Some(path) = std::env::args().nth(1) {
        return diagram(&path);
    }
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

fn diagram(path: &str) -> Result<(), String> {
    use pdfcraft_edit::AddedText;
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(path).map_err(|e| e.to_string())?,
    ));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    if reader.info().bit_depth != png::BitDepth::Eight {
        return Err("Le diagnostic attend une image 8 bits".into());
    }
    if reader.info().width > 4096 || reader.info().height > 4096 {
        return Err("Image trop grande".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("Image trop grande")?];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let source = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => source.to_vec(),
        png::ColorType::Rgb => source
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err("Le diagnostic attend une image RGB ou RGBA 8 bits".into()),
    };
    let result = pdfinator::ocr::recognize(&rgba, info.width, info.height)?;
    let mut blocks = result.styled_blocks(&rgba, info.width, info.height);
    let background = pdfinator::ocr::erase_letters(&rgba, info.width, info.height, &mut blocks)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &blocks
                .iter()
                .map(|b| serde_json::json!({
                    "text":b.text,"size":b.size,"bold":b.bold,"rect":b.rect
                }))
                .collect::<Vec<_>>()
        )
        .map_err(|e| e.to_string())?
    );
    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "test-output/diagram-ocr".into()),
    );
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let png = pdfinator::ocr::encode_png(&background, info.width, info.height)?;
    let doc = pdfcraft_create::blank(info.width as f64, info.height as f64, 1)
        .map_err(|e| e.to_string())?;
    let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).map_err(|e| e.to_string())?;
    let mut editor = pdfinator::document::Editor::from_bytes(bytes)?;
    editor.image_bytes(
        0,
        [0., 0., info.width as f64, info.height as f64],
        "diagnostic.png",
        &pdfinator::ocr::encode_png(&rgba, info.width, info.height)?,
        None,
    )?;
    editor.image_to_blocks(
        0,
        pdfinator::ocr::ImageSource::Added(0),
        &png,
        [0., 0., info.width as f64, info.height as f64],
        blocks
            .iter()
            .map(|b| AddedText {
                rect: [
                    b.rect[0] as f64,
                    info.height as f64 - b.rect[3] as f64,
                    b.rect[2] as f64 + 4.,
                    info.height as f64 - b.rect[1] as f64,
                ],
                text: b.text.clone(),
                size: b.size,
                bold: b.bold,
                color: b.color,
                ..Default::default()
            })
            .collect(),
    )?;
    std::fs::write(output.join("converted.pdf"), &*editor.bytes).map_err(|e| e.to_string())?;
    let mut renderer = pdfcraft_render::PageRenderer::new(editor.bytes.clone(), Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page: 0,
        scale: 1.,
        ..Default::default()
    });
    if let Some(e) = image.error {
        return Err(e);
    }
    std::fs::write(
        output.join("converted.png"),
        pdfinator::ocr::encode_png(&image.rgba, image.width, image.height)?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
