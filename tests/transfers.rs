use pdfcraft_cos::{Document, Object, SaveOptions};
use pdfcraft_edit::{AddedText, BlockStyle};
use pdfinator::{
    document::Editor,
    rich::{self, Format, TextData},
};

fn bytes(doc: &Document) -> Vec<u8> {
    pdfcraft_cos::write_full(doc, &SaveOptions::default()).unwrap()
}
fn document(text: &str) -> Editor {
    Editor::from_bytes(bytes(
        &pdfcraft_create::from_text("Transfert", text, (595., 842.), 14.).unwrap(),
    ))
    .unwrap()
}
fn blank(rotation: i64) -> Editor {
    let mut doc = pdfcraft_create::blank(595., 842., 1).unwrap();
    let p = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(p, |d| d.set(b"Rotate".to_vec(), Object::Int(rotation)))
        .unwrap();
    Editor::from_bytes(bytes(&doc)).unwrap()
}
fn output(editor: &mut Editor, name: &str) -> Editor {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    editor.save(&path).unwrap();
    Editor::open(&path).unwrap()
}
#[test]
fn copied_images_keep_crop_pixels_and_direction_across_rotated_documents() {
    use pdfcraft_edit::{ObjectKind, ObjectTarget};
    use pdfinator::ocr::ImageSource;
    let mut pixels = vec![0; 24 * 16 * 4];
    for y in 0..16 {
        for x in 0..24 {
            let color = if x < 12 {
                [230, 30, 20, 255]
            } else if y < 8 {
                [20, 180, 60, 255]
            } else {
                [20, 40, 230, 255]
            };
            pixels[(y * 24 + x) * 4..(y * 24 + x + 1) * 4].copy_from_slice(&color);
        }
    }
    let png = pdfinator::ocr::encode_png(&pixels, 24, 16).unwrap();
    for rotation in [0, 90, 180, 270] {
        let mut source = blank(rotation);
        source
            .image_bytes(0, [70., 120., 310., 280.], "image.png", &png, None)
            .unwrap();
        source
            .add_text(
                0,
                AddedText {
                    rect: [350., 350., 500., 380.],
                    text: "Texte voisin intact".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        source
            .crop_image(0, ImageSource::Added(0), [10., 20., 15., 5.])
            .unwrap();
        let before = source.bytes.clone();
        let copied = source
            .copy_image_block(
                0,
                ObjectTarget {
                    kind: ObjectKind::Image,
                    index: 0,
                },
            )
            .unwrap();
        assert_eq!(source.bytes, before);
        let preview = copied.clipboard_preview().unwrap();
        assert!(preview.error.is_none());
        assert_eq!(preview.width, 360);
        assert_eq!(preview.height, 240);
        for dest_rotation in [0, 90, 180, 270] {
            let mut destination = blank(dest_rotation);
            let original = destination.bytes.clone();
            let pasted = destination
                .paste_image_block(0, &copied, [90., 100.])
                .unwrap();
            let (_, area) = destination
                .image_for_ocr(0, ImageSource::Original(pasted))
                .unwrap();
            for (a, b) in area.iter().zip([90., 100., 270., 220.]) {
                assert!(
                    (*a - b).abs() < 0.01,
                    "{rotation} → {dest_rotation}: {area:?}"
                );
            }
            let mut renderer =
                pdfcraft_render::PageRenderer::new(destination.bytes.clone(), Default::default());
            let page = renderer.render(pdfcraft_render::RenderRequest {
                page: 0,
                scale: 1.,
                ..Default::default()
            });
            assert!(page.error.is_none());
            let pixel = |x, y| &page.rgba[((y * page.width + x) * 4) as usize..][..3];
            assert_eq!(pixel(100, 110), [230, 30, 20]);
            assert_eq!(pixel(250, 110), [20, 180, 60]);
            assert_eq!(pixel(250, 200), [20, 40, 230]);
            assert!(!destination.text(0).unwrap().plain_text().contains("voisin"));
            let copied_again = destination
                .copy_image_block(
                    0,
                    ObjectTarget {
                        kind: ObjectKind::Image,
                        index: pasted,
                    },
                )
                .unwrap();
            let second = destination
                .paste_image_block(0, &copied_again, [300., 280.])
                .unwrap();
            assert!(second > pasted);
            assert!(
                destination
                    .image_for_ocr(0, ImageSource::Original(second))
                    .is_ok()
            );
            destination.undo().unwrap();
            destination.undo().unwrap();
            assert_eq!(destination.bytes, original);
            destination.redo().unwrap();
            destination
                .crop_image(0, ImageSource::Original(pasted), [25., 0., 0., 0.])
                .unwrap();
            let saved = output(
                &mut destination,
                &format!("42-collage-image-{rotation}-{dest_rotation}.pdf"),
            );
            assert!(
                saved
                    .image_for_ocr(0, ImageSource::Original(pasted))
                    .is_ok()
            );
        }
    }
}
#[test]
fn copied_paragraph_preserves_embedded_font_styles_and_remains_editable() {
    let mut source = document("Un été français avec œ et l’apostrophe.");
    let b = source.blocks(0).unwrap().remove(0);
    let mut data = TextData {
        id: "source".into(),
        text: b.editor_text.clone(),
        rect: b.rect,
        base_size: 14.,
        base_color: [0., 0., 0.],
        base_bold: false,
        base_italic: false,
        font_name: "Inter".into(),
        align: Some("Centre".into()),
        line_spacing: None,
        spans: Vec::new(),
    };
    rich::apply(
        &mut data.spans,
        0..data.text.len(),
        &Format {
            resource: Some("chosen".into()),
            font_name: Some("Inter".into()),
            size: Some(14.),
            ..Default::default()
        },
    );
    let start = data.text.find("français").unwrap();
    rich::apply(
        &mut data.spans,
        start..start + "français".len(),
        &Format {
            color: Some([0.8, 0.1, 0.2]),
            underline: Some(true),
            strike: Some(true),
            ..Default::default()
        },
    );
    source
        .format_text(
            0,
            Some(0),
            None,
            data,
            &[(
                "chosen".into(),
                include_bytes!("../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf").to_vec(),
            )],
        )
        .unwrap();
    source
        .add_comment(0, [500., 700.], "Une note qui ne doit pas être copiée")
        .unwrap();
    let source_bytes = source.bytes.clone();
    let copied = source.copy_text_block(0, Some(0), None).unwrap();
    let after = copied.data.text.find("avec").unwrap();
    assert_eq!(
        copied
            .data
            .spans
            .iter()
            .find(|s| s.range.contains(&after))
            .unwrap()
            .format
            .color,
        Some([0., 0., 0.])
    );
    assert_eq!(source.bytes, source_bytes);
    let mut target = document("Destination à conserver.");
    let before = target.bytes.clone();
    let selected = target.paste_text_block(0, &copied, [80., 300.]).unwrap();
    let pdfinator::transfer::PastedText::Block(selected) = selected else {
        panic!("Expected a formatted paragraph")
    };
    let text = target.text(0).unwrap().plain_text();
    assert_eq!(text.matches("Un été français").count(), 1);
    assert!(text.contains("Destination"));
    assert!(target.info.annotations.is_empty());
    let data = target.text_formatting(0, selected).unwrap();
    assert_eq!(data.align.as_deref(), Some("Centre"));
    assert!(data.spans.iter().any(|s| s.format.strike == Some(true)
        && s.format.underline == Some(true)
        && s.format.color == Some([0.8, 0.1, 0.2])));
    let fonts = pdfcraft_edit::text::text_editor_fonts(&target.doc, 0).unwrap();
    assert!(fonts.iter().any(|f| f.resource.starts_with("PNPaste")
        && f.bytes == include_bytes!("../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf")));
    let pasted = target.bytes.clone();
    target.undo().unwrap();
    assert_eq!(target.bytes, before);
    target.redo().unwrap();
    assert_eq!(target.bytes, pasted);
    let mut reopened = output(&mut target, "20-bloc-copie.pdf");
    let index = reopened
        .blocks(0)
        .unwrap()
        .iter()
        .position(|b| b.text.contains("Un été"))
        .unwrap();
    reopened
        .rewrite(
            0,
            index,
            Some("Un été français corrigé après collage."),
            &BlockStyle::default(),
        )
        .unwrap();
    let reopened = output(&mut reopened, "20-bloc-copie-corrige.pdf");
    assert_eq!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .matches("corrigé après collage")
            .count(),
        1
    );
    assert_eq!(source.bytes, source_bytes);
}
#[test]
fn added_text_can_be_copied_to_rotated_pages_and_undone_once() {
    let mut source = blank(0);
    source
        .add_text(
            0,
            AddedText {
                rect: [60., 600., 360., 670.],
                text: "Été : é è ê à ç œ, l’apostrophe.".into(),
                family: pdfcraft_edit::Family::Times,
                color: [0.1, 0.4, 0.7],
                italic: true,
                size: 16.,
                ..Default::default()
            },
        )
        .unwrap();
    let copied = source.copy_text_block(0, None, Some(0)).unwrap();
    for rotation in [0, 90, 180, 270] {
        let mut target = blank(rotation);
        let before = target.bytes.clone();
        target.paste_text_block(0, &copied, [70., 200.]).unwrap();
        let text = target.text(0).unwrap().plain_text();
        assert!(text.contains("Été : é è ê à ç œ"), "{rotation}: {text}");
        let pdfcraft_edit::Content::Text(b) = target.added(0).remove(0) else {
            panic!("Text expected")
        };
        assert_eq!(b.family, pdfcraft_edit::Family::Times);
        target.undo().unwrap();
        assert_eq!(target.bytes, before);
        assert!(!target.can_undo());
        target.redo().unwrap();
        let reopened = output(&mut target, &format!("20-ajout-colle-{rotation}.pdf"));
        assert_eq!(
            reopened
                .text(0)
                .unwrap()
                .plain_text()
                .matches("Été")
                .count(),
            1
        );
    }
}
#[test]
fn append_several_pdfs_preserves_pages_annotations_rotation_and_history() {
    let mut destination = document("Document principal.");
    let before = destination.bytes.clone();
    let mut first = document("Premier document ajouté.");
    let page_text = first.text(0).unwrap();
    first.highlight(0, &page_text, 0..7).unwrap();
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[20, 100, 220, 255, 20, 200, 180, 0])
            .unwrap();
    }
    first
        .image_bytes(
            0,
            [60., 400., 200., 470.],
            "transparent.png",
            &png_bytes,
            None,
        )
        .unwrap();
    first
        .add_comment(0, [400., 500.], "Commentaire conservé")
        .unwrap();
    first.rotate_page(0, 90).unwrap();
    let mut second =
        Editor::from_bytes(bytes(&pdfcraft_create::blank(420., 595., 2).unwrap())).unwrap();
    for page in 0..2 {
        second
            .add_text(
                page,
                AddedText {
                    text: format!("Second document page {}", page + 1),
                    rect: [40., 400., 380., 470.],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let first_bytes = first.bytes.clone();
    let second_bytes = second.bytes.clone();
    assert_eq!(
        destination
            .append_pdfs(&[
                ("premier.pdf", (*first.bytes).clone()),
                ("second.pdf", (*second.bytes).clone())
            ])
            .unwrap(),
        3
    );
    assert_eq!(destination.info.pages.len(), 4);
    assert_eq!(
        pdfcraft_edit::page_images(&destination.doc, 1)
            .unwrap()
            .len(),
        1
    );
    assert!(
        destination
            .info
            .annotations
            .iter()
            .any(|a| a.page == 1 && a.subtype == "Highlight")
    );
    assert_eq!(destination.info.pages[1].rotation, 90);
    for (page, text) in [
        "principal",
        "Premier document",
        "Second document page 1",
        "Second document page 2",
    ]
    .iter()
    .enumerate()
    {
        assert!(destination.text(page).unwrap().plain_text().contains(text));
    }
    assert!(
        destination
            .info
            .annotations
            .iter()
            .any(|a| a.page == 1 && a.contents.as_deref() == Some("Commentaire conservé"))
    );
    let combined = destination.bytes.clone();
    destination.undo().unwrap();
    assert_eq!(destination.bytes, before);
    assert!(!destination.can_undo());
    destination.redo().unwrap();
    assert_eq!(destination.bytes, combined);
    let mut reopened = output(&mut destination, "21-pdf-concatenes.pdf");
    assert_eq!(reopened.info.pages.len(), 4);
    reopened.move_page(3, 0).unwrap();
    reopened.rotate_page(1, 90).unwrap();
    let reopened = output(&mut reopened, "21-pdf-concatenes-ordonnes.pdf");
    assert!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .contains("Second document page 2")
    );
    assert_eq!(first.bytes, first_bytes);
    assert_eq!(second.bytes, second_bytes);
}
#[test]
fn failed_append_or_paste_never_leaves_a_partial_document() {
    let mut destination = document("À préserver.");
    let before = destination.bytes.clone();
    let valid = document("Valide");
    assert!(
        destination
            .append_pdfs(&[
                ("valide.pdf", (*valid.bytes).clone()),
                ("invalide.pdf", b"broken PDF".to_vec())
            ])
            .is_err()
    );
    assert_eq!(destination.bytes, before);
    assert!(!destination.can_undo());
    let mut copied = valid.copy_text_block(0, Some(0), None).unwrap();
    copied.data.base_size = f64::NAN;
    assert!(
        destination
            .paste_text_block(0, &copied, [50., 100.])
            .is_err()
    );
    assert_eq!(destination.bytes, before);
    assert!(!destination.can_undo());
    assert_eq!(destination.append_pdfs(&[]).unwrap(), 0);
    assert!(!destination.can_undo());
}

#[test]
fn source_fonts_never_overwrite_the_destination_and_rotated_rich_paste_is_refused_atomically() {
    let source = document("Un texte de départ.");
    let copied = source.copy_text_block(0, Some(0), None).unwrap();
    let mut target = document("Texte cible intact.");
    let p = pdfcraft_model::pages(&target.doc)[0].clone();
    let fonts_before = p
        .dict
        .get(b"Resources")
        .and_then(|r| target.doc.dict(r))
        .and_then(|r| r.get(b"Font").and_then(|r| target.doc.dict(r)))
        .unwrap();
    target.paste_text_block(0, &copied, [60., 300.]).unwrap();
    target.paste_text_block(0, &copied, [60., 500.]).unwrap();
    assert_eq!(
        target
            .text(0)
            .unwrap()
            .plain_text()
            .matches("Un texte de départ")
            .count(),
        2
    );
    let p = pdfcraft_model::pages(&target.doc)[0].clone();
    let fonts_after = p
        .dict
        .get(b"Resources")
        .and_then(|r| target.doc.dict(r))
        .and_then(|r| r.get(b"Font").and_then(|r| target.doc.dict(r)))
        .unwrap();
    for (key, value) in fonts_before.iter() {
        assert_eq!(fonts_after.get(key), Some(value));
    }
    for rotation in [90, 180, 270] {
        let mut target = blank(rotation);
        let before = target.bytes.clone();
        assert!(
            target
                .paste_text_block(0, &copied, [60., 300.])
                .unwrap_err()
                .contains("page tournée")
        );
        assert_eq!(target.bytes, before);
        assert!(!target.can_undo());
    }
}
