use pdfcraft_cos::{Document, Object, SaveOptions};
use pdfcraft_edit::{AddedText, BlockStyle, Content};
use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest};
use pdfinator::document::Editor;
use std::path::PathBuf;

fn serialize(d: &Document) -> Vec<u8> {
    pdfcraft_cos::write_full(d, &SaveOptions::default()).unwrap()
}
fn source() -> Editor {
    let doc = pdfcraft_create::from_text(
        "Essai PDFinator",
        "Une phrase originale à corriger.",
        (595.0, 842.0),
        14.0,
    )
    .unwrap();
    Editor::from_bytes(serialize(&doc)).unwrap()
}
fn output(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}
fn reopen(e: &mut Editor, name: &str) -> Editor {
    let path = output(name);
    e.save(&path).unwrap();
    Editor::open(&path).unwrap()
}
fn plain(e: &Editor) -> String {
    e.text(0).unwrap().plain_text()
}
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 128, 255, 255, 0, 255,
            ])
            .unwrap();
    }
    bytes
}

#[test]
fn image_crop_preserves_other_images_and_survives_rotation_save_and_undo() {
    use pdfinator::ocr::ImageSource;
    for rotation in [0_i64, 90, 180, 270] {
        for original in [false, true] {
            let mut d = pdfcraft_create::blank(595., 842., 1).unwrap();
            let page = pdfcraft_model::pages(&d)[0].obj;
            d.update_dict(page, |p| {
                p.set(b"Rotate".to_vec(), Object::Int(rotation));
            })
            .unwrap();
            let mut e = Editor::from_bytes(serialize(&d)).unwrap();
            e.add_text(
                0,
                AddedText {
                    rect: [10., 10., 40., 30.],
                    text: "Note".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            e.image_bytes(0, [70., 120., 400., 500.], "crop.png", &png(), None)
                .unwrap();
            e.image_bytes(0, [430., 120., 500., 200.], "keep.png", &png(), None)
                .unwrap();
            if original {
                let image = pdfcraft_edit::list_added(&e.doc)
                    .into_iter()
                    .filter(|a| a.page == 0)
                    .nth(1)
                    .unwrap()
                    .obj;
                e.doc
                    .update_dict(image, |d| {
                        d.remove(b"PCMark");
                        d.remove(b"PCAdded");
                    })
                    .unwrap();
            }
            e = Editor::from_bytes(serialize(&e.doc)).unwrap();
            let source = if original {
                ImageSource::Original(0)
            } else {
                ImageSource::Added(1)
            };
            let (_, area) = e.image_for_ocr(0, source).unwrap();
            let untouched = pdfcraft_edit::page_images(&e.doc, 0).unwrap()[1].visible_rect;
            let before = e.bytes.clone();
            assert!(e.crop_image(0, source, [80., 0., 30., 0.]).is_err());
            assert_eq!(e.bytes, before);
            e.crop_image(0, source, [10., 20., 30., 5.]).unwrap();
            let (_, cropped) = e.image_for_ocr(0, ImageSource::Original(0)).unwrap();
            let w = area[2] - area[0];
            let h = area[3] - area[1];
            let expected = [
                area[0] + w * 0.1,
                area[1] + h * 0.2,
                area[2] - w * 0.3,
                area[3] - h * 0.05,
            ];
            assert!(
                cropped
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (*a - b).abs() < 0.01),
                "rotation {rotation}, added {}: {cropped:?} vs {expected:?}",
                !original
            );
            assert_eq!(
                pdfcraft_edit::page_images(&e.doc, 0).unwrap()[1].visible_rect,
                untouched
            );
            assert!(plain(&e).contains("Note"));
            let after = e.bytes.clone();
            e.undo().unwrap();
            assert_eq!(e.bytes, before);
            e.redo().unwrap();
            assert_eq!(e.bytes, after);
            let e = reopen(&mut e, &format!("41-crop-{rotation}-{original}.pdf"));
            let (_, saved) = e.image_for_ocr(0, ImageSource::Original(0)).unwrap();
            assert!(
                saved
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (*a - b).abs() < 0.01)
            );
        }
    }
}

#[test]
fn image_conversion_is_atomic_reeditable_and_removes_the_drawing_at_rotations() {
    use pdfinator::ocr::ImageSource;
    for rotation in [0_i64, 90, 180, 270] {
        for original in [false, true] {
            let mut d = pdfcraft_create::blank(595., 842., 2).unwrap();
            let page = pdfcraft_model::pages(&d)[0].obj;
            d.update_dict(page, |p| {
                p.set(b"Rotate".to_vec(), Object::Int(rotation));
            })
            .unwrap();
            let mut e = Editor::from_bytes(serialize(&d)).unwrap();
            e.image_bytes(0, [70., 120., 400., 500.], "scan.png", &png(), None)
                .unwrap();
            e.add_text(
                1,
                AddedText {
                    rect: [40., 700., 500., 740.],
                    text: "Page suivante conservée".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            e.add_comment(0, [20., 20.], "Note conservée").unwrap();
            if original {
                let image = pdfcraft_edit::list_added(&e.doc)
                    .into_iter()
                    .find(|a| a.page == 0)
                    .unwrap()
                    .obj;
                e.doc
                    .update_dict(image, |d| {
                        d.remove(b"PCMark");
                        d.remove(b"PCAdded");
                    })
                    .unwrap();
                e = Editor::from_bytes(serialize(&e.doc)).unwrap();
            } else {
                e = Editor::from_bytes((*e.bytes).clone()).unwrap();
            }
            let source = if original {
                ImageSource::Original(0)
            } else {
                ImageSource::Added(0)
            };
            let before = e.bytes.clone();
            let (isolated, area) = e.image_for_ocr(0, source).unwrap();
            assert_eq!(
                e.bytes, before,
                "Recognition preparation must not edit the PDF"
            );
            assert!(!e.can_undo());
            assert!(
                (area[0] - 70.).abs() < 1. && (area[2] - 400.).abs() < 1.,
                "View geometry at rotation {rotation}: {area:?}"
            );
            let mut renderer = PageRenderer::new(isolated, Default::default());
            let shot = renderer.render(RenderRequest {
                page: 0,
                scale: 0.5,
                ..Default::default()
            });
            assert!(shot.error.is_none());
            let recognized = "École : é, è, ê, à, ç, œ et l’été.\nTexte corrigé.";
            let index = e
                .image_to_text(
                    0,
                    source,
                    AddedText {
                        rect: [70., 120., 400., 500.],
                        text: recognized.into(),
                        size: 14.,
                        ..Default::default()
                    },
                )
                .unwrap();
            let after = e.bytes.clone();
            assert!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().is_empty());
            assert_eq!(e.info.annotations.len(), 1);
            assert_eq!(e.info.pages.len(), 2);
            e.undo().unwrap();
            assert_eq!(e.bytes, before);
            assert!(!e.can_undo(), "OCR replacement must be one undo step");
            e.redo().unwrap();
            assert_eq!(e.bytes, after);
            let mut reopened = reopen(
                &mut e,
                &format!(
                    "18-ocr-{}-{rotation}.pdf",
                    if original { "original" } else { "ajout" }
                ),
            );
            let text = plain(&reopened);
            assert_eq!(text.matches("Texte corrigé").count(), 1);
            for word in ["École", "œ", "l’été"] {
                assert!(text.contains(word), "Missing {word}: {text}");
            }
            assert!(
                reopened
                    .text(1)
                    .unwrap()
                    .plain_text()
                    .contains("suivante conservée")
            );
            let mut added = match reopened.added(0)[index].clone() {
                Content::Text(t) => t,
                _ => panic!("Expected real text"),
            };
            added.text = "Nouvelle correction après réouverture.".into();
            reopened
                .update_added(0, index, Content::Text(added))
                .unwrap();
            assert_eq!(plain(&reopened).matches("Nouvelle correction").count(), 1);
            assert!(
                pdfcraft_edit::page_images(&reopened.doc, 0)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn failed_image_conversion_keeps_the_picture_and_history_intact() {
    let mut e =
        Editor::from_bytes(serialize(&pdfcraft_create::blank(595., 842., 1).unwrap())).unwrap();
    e.image_bytes(0, [60., 200., 400., 600.], "scan.png", &png(), None)
        .unwrap();
    let before = e.bytes.clone();
    assert!(
        e.image_to_text(
            0,
            pdfinator::ocr::ImageSource::Added(0),
            AddedText {
                rect: [60., 200., 400., 600.],
                text: "\u{10ffff}".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(e.bytes, before);
    assert_eq!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().len(), 1);
    e.add_text(
        0,
        AddedText {
            rect: [80., 300., 320., 340.],
            text: "Couche existante".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let before = e.bytes.clone();
    let (isolated, area) = e
        .image_for_ocr(0, pdfinator::ocr::ImageSource::Added(0))
        .unwrap();
    let mut renderer = PageRenderer::new(isolated, Default::default());
    let output = renderer.render(RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Text,
        ..Default::default()
    });
    assert!(
        !output
            .text
            .unwrap()
            .plain_text()
            .contains("Couche existante")
    );
    assert!(!e.image_ocr_retained_text(0, area).unwrap().is_empty());
    assert_eq!(
        e.bytes, before,
        "Native text must not block isolated raster recognition"
    );
}

fn rich_data(e: &Editor) -> pdfinator::rich::TextData {
    let b = &e.blocks(0).unwrap()[0];
    pdfinator::rich::TextData {
        id: "roundtrip".into(),
        text: b.editor_text.clone(),
        rect: b.rect,
        base_size: b.size,
        base_color: b.color,
        base_bold: b.bold,
        base_italic: b.italic,
        font_name: b.base_font.clone(),
        align: None,
        line_spacing: None,
        spans: Vec::new(),
    }
}

#[test]
fn bullets_and_line_spacing_remain_native_after_resize_copy_undo_and_reopen() {
    let mut e = source();
    let mut data = rich_data(&e);
    data.text = "Premier élément\nDeuxième élément\nTroisième élément".into();
    data.rect[1] -= 100.;
    pdfinator::rich::toggle_bullets(&mut data.text, &mut data.spans, None, &mut []);
    data.line_spacing = Some(2.);
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    let baseline_gaps = |e: &Editor| {
        let lines = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
        let ys: Vec<_> = lines.iter().map(|l| l.rect[3]).collect();
        ys.windows(2)
            .map(|w| (w[0] - w[1]).abs())
            .collect::<Vec<_>>()
    };
    assert!(
        baseline_gaps(&e).iter().all(|v| (*v - 28.).abs() < 0.05),
        "{:?}",
        baseline_gaps(&e)
    );
    let saved = reopen(&mut e, "36-puces-interligne.pdf");
    assert_eq!(plain(&saved).matches('•').count(), 3);
    assert_eq!(saved.text_formatting(0, 0).unwrap().line_spacing, Some(2.));
    e.rewrite(
        0,
        0,
        None,
        &BlockStyle {
            width: Some(350.),
            offset: Some([8., -12.]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(baseline_gaps(&e).iter().all(|v| (*v - 28.).abs() < 0.05));
    let copied = e.copy_text_block(0, Some(0), None).unwrap();
    let mut target = source();
    let pasted = target.paste_text_block(0, &copied, [70., 220.]).unwrap();
    let pdfinator::transfer::PastedText::Block(index) = pasted else {
        panic!("Formatted paragraph expected")
    };
    assert_eq!(
        target.text_formatting(0, index).unwrap().line_spacing,
        Some(2.)
    );
    reopen(&mut target, "36-puces-copie.pdf");
    let before = e.bytes.clone();
    e.rewrite(
        0,
        0,
        None,
        &BlockStyle {
            line_spacing: Some(1.25),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(baseline_gaps(&e).iter().all(|v| (*v - 17.5).abs() < 0.05));
    reopen(&mut e, "36-puces-interligne-125.pdf");
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}

#[test]
fn highlighter_toggles_exact_or_partial_text_without_erasing_neighbours() {
    for rotation in [0, 90, 180, 270] {
        let mut e = source();
        if rotation != 0 {
            e.rotate_page(0, rotation).unwrap();
        }
        let text = e.text(0).unwrap();
        let original = e.bytes.clone();
        e.toggle_highlight(0, &text, 0..16).unwrap();
        assert_eq!(e.info.annotations.len(), 1);
        e.toggle_highlight(0, &text, 0..4).unwrap();
        assert_eq!(e.info.annotations.len(), 1);
        reopen(&mut e, &format!("37-surlignage-partiel-{rotation}.pdf"));
        e.toggle_highlight(0, &text, 4..16).unwrap();
        assert!(e.info.annotations.is_empty());
        e.undo().unwrap();
        assert_eq!(e.info.annotations.len(), 1);
        e.undo().unwrap();
        assert_eq!(e.info.annotations.len(), 1);
        e.undo().unwrap();
        assert_eq!(e.bytes, original);
        e.toggle_highlight(0, &text, 0..16).unwrap();
        e.toggle_highlight(0, &text, 0..16).unwrap();
        assert!(e.info.annotations.is_empty());
    }
}

#[test]
fn mixed_character_formatting_survives_repeated_edit_and_reopen() {
    use pdfinator::rich::{Format, apply};
    let mut e = source();
    let mut data = rich_data(&e);
    let start = data.text.find("phrase").unwrap();
    apply(
        &mut data.spans,
        start..start + 6,
        &Format {
            color: Some([0.8, 0.1, 0.2]),
            strike: Some(true),
            underline: Some(true),
            highlight: Some(true),
            ..Default::default()
        },
    );
    e.format_text(0, Some(0), None, data.clone(), &[]).unwrap();
    let mut e = reopen(&mut e, "12-formatage.pdf");
    assert_eq!(e.blocks(0).unwrap().len(), 1);
    assert_eq!(plain(&e).matches("phrase").count(), 1);
    let saved = e.text_formatting(0, 0).unwrap();
    assert_eq!(saved.spans, data.spans);
    let lines = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    assert!(
        lines
            .iter()
            .any(|l| l.text.contains("phrase") && l.color == [0.8, 0.1, 0.2])
    );
    data.text = "Une phrase corrigée à Paris.".into();
    e.format_text(0, Some(0), None, data.clone(), &[]).unwrap();
    let e = reopen(&mut e, "12-formatage-reedite.pdf");
    assert_eq!(plain(&e).matches("corrigée").count(), 1);
    assert!(!plain(&e).contains("originale"));
    let preview = Editor::from_bytes(
        e.preview_without(0, Some(0), None)
            .unwrap()
            .as_ref()
            .clone(),
    )
    .unwrap();
    assert!(plain(&preview).trim().is_empty());
}

#[test]
fn formatted_container_keeps_blank_lines_as_one_editable_block() {
    let mut e = source();
    let mut data = rich_data(&e);
    data.text = "Première ligne.\n\nDernière ligne.".into();
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    assert_eq!(e.blocks(0).unwrap().len(), 1);
    let mut data = e.text_formatting(0, 0).unwrap();
    data.text = "Première ligne.\n\nDernière ligne corrigée.".into();
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    assert_eq!(plain(&e).matches("Première").count(), 1);
    assert_eq!(plain(&e).matches("corrigée").count(), 1);
    reopen(&mut e, "19-paragraphe-espace.pdf");
}

#[test]
fn moved_formatted_text_keeps_its_position_on_later_edits() {
    let mut e = source();
    let mut data = rich_data(&e);
    data.text = "\nPremière ligne.\n\nDernière ligne.".into();
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    e.move_block(0, 0, [30., -25.]).unwrap();
    let before = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    let data = e.text_formatting(0, 0).unwrap();
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    let after = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        for axis in 0..4 {
            assert!(
                (a.rect[axis] - b.rect[axis]).abs() < 0.05,
                "{:?} {:?}",
                a.rect,
                b.rect
            );
        }
    }
    reopen(&mut e, "20-texte-deplace-reedite.pdf");
}

#[test]
fn superscript_and_subscript_remain_one_editable_paragraph() {
    use pdfinator::rich::{Format, apply};
    let mut e = source();
    let mut data = rich_data(&e);
    data.text = "x2 et H2O".into();
    apply(
        &mut data.spans,
        1..2,
        &Format {
            script: Some(1),
            ..Default::default()
        },
    );
    apply(
        &mut data.spans,
        7..8,
        &Format {
            script: Some(-1),
            ..Default::default()
        },
    );
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    let mut e = reopen(&mut e, "13-exposant-indice.pdf");
    assert_eq!(
        e.blocks(0).unwrap().len(),
        1,
        "superscript must belong to the surrounding paragraph"
    );
    let data = e.text_formatting(0, 0).unwrap();
    assert_eq!(data.text, "x2 et H2O");
    let before = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    let after = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(after.iter()) {
        assert!(
            (a.rect[1] - b.rect[1]).abs() < 0.2,
            "{:?} {:?}",
            a.rect,
            b.rect
        );
    }
}

#[test]
fn named_embedded_font_and_reflow_keep_glyph_size() {
    use pdfinator::rich::{Format, apply};
    let mut e = source();
    let mut data = rich_data(&e);
    data.text =
        "é è ê à ç œ l’été : une phrase suffisamment longue pour revenir à la ligne.".into();
    data.rect[2] = data.rect[0] + 170.;
    apply(
        &mut data.spans,
        0..data.text.len(),
        &Format {
            resource: Some("inter".into()),
            font_name: Some("Inter".into()),
            ..Default::default()
        },
    );
    e.format_text(
        0,
        Some(0),
        None,
        data.clone(),
        &[(
            "inter".into(),
            include_bytes!("../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf").to_vec(),
        )],
    )
    .unwrap();
    let mut e = reopen(&mut e, "14-police-reflow.pdf");
    let lines = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    assert!(lines.len() > 1);
    assert!(lines.iter().all(|l| (l.size - 14.).abs() < 0.05));
    assert_eq!(e.text_formatting(0, 0).unwrap().text, data.text);
    let mut data = e.text_formatting(0, 0).unwrap();
    data.rect[2] = data.rect[0] + 340.;
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    let e = reopen(&mut e, "14-police-reflow-large.pdf");
    let wider = pdfcraft_edit::text_lines(&e.doc, 0).unwrap();
    let rows = |runs: &[pdfcraft_edit::TextLine]| {
        let mut positions: Vec<_> = runs
            .iter()
            .map(|l| (l.baseline()[1] * 10.).round() as i64)
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions.len()
    };
    assert!(
        rows(&wider) < rows(&lines),
        "{} rows before, {} after",
        rows(&lines),
        rows(&wider)
    );
    assert!(wider.iter().all(|l| (l.size - 14.).abs() < 0.05));
}

#[test]
fn original_text_container_reflows_instead_of_scaling_characters() {
    let mut e = source();
    e.rewrite(
        0,
        0,
        None,
        &BlockStyle {
            width: Some(85.),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(e.blocks(0).unwrap()[0].rows.len() > 1);
    assert!(
        pdfcraft_edit::text_lines(&e.doc, 0)
            .unwrap()
            .iter()
            .all(|l| (l.size - 14.).abs() < 0.05)
    );
    e.rewrite(
        0,
        0,
        None,
        &BlockStyle {
            width: Some(400.),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(e.blocks(0).unwrap()[0].rows.len(), 1);
    assert_eq!(plain(&e).matches("originale").count(), 1);
    reopen(&mut e, "18-redimensionnement-source.pdf");
}

#[test]
fn comments_are_real_pdf_annotations_with_history_and_roundtrip() {
    let mut e = source();
    let i = e
        .add_comment(0, [80., 700.], "Une remarque à propos de l’été.")
        .unwrap();
    e.set_comment(0, i, "Vérifier cette phrase, s’il vous plaît.")
        .unwrap();
    e.undo().unwrap();
    assert!(
        e.info
            .annotations
            .iter()
            .any(|a| a.contents.as_deref() == Some("Une remarque à propos de l’été."))
    );
    e.redo().unwrap();
    let e = reopen(&mut e, "15-commentaire.pdf");
    assert!(e.info.annotations.iter().any(|a| a.subtype == "Text"
        && a.contents.as_deref() == Some("Vérifier cette phrase, s’il vous plaît.")));
}
#[test]
fn multipage_render_search_zoom() {
    let mut d = pdfcraft_create::blank(595.0, 842.0, 3).unwrap();
    for page in 0..3 {
        pdfcraft_edit::add_content(
            &mut d,
            page,
            &Content::Text(AddedText {
                rect: [50.0, 700.0, 500.0, 740.0],
                text: format!("Page {} : été à Paris", page + 1),
                ..Default::default()
            }),
        )
        .unwrap();
    }
    let mut e = Editor::from_bytes(serialize(&d)).unwrap();
    assert_eq!(e.info.pages.len(), 3);
    let mut r = PageRenderer::new(e.bytes.clone(), RenderConfig::default());
    for page in 0..3 {
        assert_eq!(e.text(page).unwrap().find("Paris").len(), 1);
        for scale in [0.5, 1.0, 1.5, 2.0] {
            let image = r.render(RenderRequest {
                page,
                scale,
                ..Default::default()
            });
            assert!(image.error.is_none(), "{:?}", image.error);
            assert_eq!(
                image.rgba.len(),
                image.width as usize * image.height as usize * 4
            );
        }
    }
    reopen(&mut e, "01-multipage.pdf");
}
#[test]
fn rewrite_existing_roundtrip_no_duplicate() {
    let mut e = source();
    let before = e.blocks(0).unwrap()[0].rect;
    e.rewrite(
        0,
        0,
        Some("Une phrase corrigée à Paris."),
        &BlockStyle::default(),
    )
    .unwrap();
    let e = reopen(&mut e, "02-correction.pdf");
    let text = plain(&e);
    assert!(text.contains("corrigée"), "{text}");
    assert!(!text.contains("originale"));
    assert_eq!(text.matches("corrigée").count(), 1);
    let after = e.blocks(0).unwrap()[0].rect;
    assert!((before[0] - after[0]).abs() < 0.1);
    assert!((before[3] - after[3]).abs() < 0.1);
}
#[test]
fn move_source_roundtrip_no_duplicate() {
    let mut e = source();
    let before = e.blocks(0).unwrap()[0].rect;
    e.move_block(0, 0, [35.0, -45.0]).unwrap();
    let e = reopen(&mut e, "03-deplacement.pdf");
    assert_eq!(plain(&e).matches("originale").count(), 1);
    let after = e.blocks(0).unwrap()[0].rect;
    assert!((after[0] - before[0] - 35.0).abs() < 0.1);
    assert!((after[3] - before[3] + 45.0).abs() < 0.1);
}
#[test]
fn added_french_reeditable_after_reopen() {
    let mut e = source();
    let phrase = "é è ê à ç œ l’été aujourd'hui";
    e.add_text(
        0,
        AddedText {
            rect: [50.0, 500.0, 500.0, 550.0],
            text: phrase.into(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut e = reopen(&mut e, "04-francais.pdf");
    assert!(plain(&e).contains(phrase), "{}", plain(&e));
    let Content::Text(mut t) = e.added(0)[0].clone() else {
        panic!()
    };
    t.text = "Réouverture réussie".into();
    e.update_added(0, 0, Content::Text(t)).unwrap();
    let e = reopen(&mut e, "04-francais-reedite.pdf");
    assert!(plain(&e).contains("réussie"));
}
#[test]
fn transparent_image_move_resize_replace_delete() {
    let mut e = source();
    let bytes = png();
    std::fs::write(output("transparent.png"), &bytes).unwrap();
    e.image_bytes(0, [50.0, 300.0, 150.0, 400.0], "test.png", &bytes, None)
        .unwrap();
    let item = e.added(0)[0].clone();
    e.update_added(0, 0, item.with_rect([80.0, 250.0, 280.0, 450.0]))
        .unwrap();
    let mut e = reopen(&mut e, "05-image.pdf");
    let Content::Image(image) = e.added(0)[0].clone() else {
        panic!()
    };
    assert_eq!(image.rect, [80.0, 250.0, 280.0, 450.0]);
    let obj = e.doc.get(image.image);
    assert!(obj.as_dict().unwrap().get(b"SMask").is_some());
    e.image_bytes(0, image.rect, "replace.png", &bytes, Some(0))
        .unwrap();
    assert_eq!(e.added(0).len(), 1);
    e.delete_added(0, 0).unwrap();
    assert!(reopen(&mut e, "05-image-supprimee.pdf").added(0).is_empty());
}
#[test]
fn highlight_is_annotation_after_reopen() {
    let mut e = source();
    let text = e.text(0).unwrap();
    let range = text.find("phrase")[0].clone();
    e.highlight(0, &text, range).unwrap();
    let mut e = reopen(&mut e, "06-surlignage.pdf");
    let a = e
        .info
        .annotations
        .iter()
        .find(|a| a.subtype == "Highlight")
        .unwrap();
    assert!(!a.quads.is_empty());
    let index = a.index;
    e.delete_highlight(0, index).unwrap();
    assert!(
        reopen(&mut e, "06-surlignage-supprime.pdf")
            .info
            .annotations
            .is_empty()
    );
}
#[test]
fn history_saved_state_and_branch() {
    let mut e = source();
    let original = e.bytes.clone();
    e.rewrite(0, 0, Some("Première modification"), &BlockStyle::default())
        .unwrap();
    let first = e.bytes.clone();
    e.rewrite(0, 0, Some("Deuxième modification"), &BlockStyle::default())
        .unwrap();
    e.undo().unwrap();
    assert_eq!(e.bytes, first);
    e.undo().unwrap();
    assert_eq!(e.bytes, original);
    assert!(!e.dirty());
    e.redo().unwrap();
    assert_eq!(e.bytes, first);
    e.save(&output("07-historique.pdf")).unwrap();
    assert!(!e.dirty());
    e.undo().unwrap();
    assert!(e.dirty());
    e.redo().unwrap();
    assert!(!e.dirty());
    e.undo().unwrap();
    e.rewrite(0, 0, Some("Nouvelle branche"), &BlockStyle::default())
        .unwrap();
    assert!(!e.can_redo());
}
#[test]
fn scan_has_no_editable_text_and_no_silent_cover() {
    let d = pdfcraft_create::from_images(&[("scan.png".into(), png())]).unwrap();
    let mut e = Editor::from_bytes(serialize(&d)).unwrap();
    assert!(e.blocks(0).unwrap().is_empty());
    let before = e.bytes.clone();
    assert!(
        e.rewrite(0, 0, Some("Ne pas recouvrir"), &BlockStyle::default())
            .is_err()
    );
    assert_eq!(before, e.bytes);
    reopen(&mut e, "08-scan.pdf");
}
#[test]
fn invisible_text_is_rejected_atomically() {
    let mut e = source();
    let page = pdfcraft_model::pages(&e.doc)[0].clone();
    let content = page.dict.reference(b"Contents").unwrap();
    let original = e.doc.get(content);
    let Object::Stream(stream) = original.as_ref() else {
        panic!()
    };
    let mut bytes = b"3 Tr\n".to_vec();
    bytes.extend(stream.decoded().unwrap());
    e.doc.set(
        content,
        pdfcraft_cos::Stream::flate(pdfcraft_cos::Dict::new(), &bytes),
    );
    let mut e = Editor::from_bytes(serialize(&e.doc)).unwrap();
    assert!(e.blocks(0).unwrap()[0].invisible);
    let before = e.bytes.clone();
    assert!(
        e.rewrite(0, 0, Some("Interdit"), &BlockStyle::default())
            .unwrap_err()
            .contains("invisible")
    );
    assert_eq!(e.bytes, before);
}

#[test]
fn rich_formatting_cannot_remove_text_used_as_a_clipping_mask() {
    let mut e = source();
    let page = pdfcraft_model::pages(&e.doc)[0].clone();
    let content = page.dict.reference(b"Contents").unwrap();
    let object = e.doc.get(content);
    let Object::Stream(stream) = object.as_ref() else {
        panic!()
    };
    let mut bytes = b"4 Tr\n".to_vec();
    bytes.extend(stream.decoded().unwrap());
    e.doc.set(
        content,
        pdfcraft_cos::Stream::flate(pdfcraft_cos::Dict::new(), &bytes),
    );
    let mut e = Editor::from_bytes(serialize(&e.doc)).unwrap();
    let data = rich_data(&e);
    let before = e.bytes.clone();
    assert!(e.format_text(0, Some(0), None, data, &[]).is_err());
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
}
#[test]
fn source_font_preserved_and_missing_glyph_refused() {
    let mut e = source();
    let font = e.blocks(0).unwrap()[0].base_font.clone();
    e.rewrite(0, 0, Some("Même police : été"), &BlockStyle::default())
        .unwrap();
    assert_eq!(e.blocks(0).unwrap()[0].base_font, font);
    let before = e.bytes.clone();
    let result = e.rewrite(0, 0, Some("Texte \u{10ffff}"), &BlockStyle::default());
    assert!(result.is_err());
    assert_eq!(e.bytes, before);
    reopen(&mut e, "09-police.pdf");
}
#[test]
fn subset_never_silently_substitutes() {
    let mut d = pdfcraft_create::blank(595.0, 842.0, 1).unwrap();
    let page = pdfcraft_model::pages(&d)[0].obj;
    let mut font = pdfcraft_cos::Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("TrueType"));
    font.set(b"BaseFont".to_vec(), Object::name("ABCDEF+TestSubset"));
    font.set(b"FirstChar".to_vec(), 97);
    font.set(b"LastChar".to_vec(), 99);
    font.set(
        b"Widths".to_vec(),
        Object::Array(vec![500.into(), 520.into(), 0.into()]),
    );
    font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    let mut fonts = pdfcraft_cos::Dict::new();
    fonts.set(b"F1".to_vec(), Object::Ref(d.add(font)));
    let mut resources = pdfcraft_cos::Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let stream = d.add(pdfcraft_cos::Stream::flate(
        pdfcraft_cos::Dict::new(),
        b"BT /F1 14 Tf 50 700 Td (ab) Tj ET",
    ));
    d.update_dict(page, |p| {
        p.set(b"Resources".to_vec(), Object::Dict(resources));
        p.set(b"Contents".to_vec(), Object::Ref(stream));
    })
    .unwrap();
    let mut e = Editor::from_bytes(serialize(&d)).unwrap();
    let before = e.bytes.clone();
    assert!(
        e.rewrite(0, 0, Some("abc"), &BlockStyle::default())
            .is_err()
    );
    assert_eq!(e.bytes, before);
    e.rewrite(
        0,
        0,
        Some("abc"),
        &BlockStyle {
            family: Some((pdfcraft_edit::Family::Helvetica, false, false)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(plain(&reopen(&mut e, "09-subset-explicite.pdf")).contains("abc"));
}
#[test]
fn embedded_font_keeps_program_after_edit_and_reopen() {
    let mut e = source();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vendor/pdfcraft/assets/fonts/Inter-Regular.ttf");
    let program = std::fs::read(&path).unwrap();
    let resource = e.import_font(0, &path).unwrap();
    e.rewrite(
        0,
        0,
        Some("Été, cœur et français"),
        &BlockStyle {
            resource: Some(resource),
            ..Default::default()
        },
    )
    .unwrap();
    let mut e = reopen(&mut e, "09-police-incorporee.pdf");
    let font = e.blocks(0).unwrap()[0].base_font.clone();
    e.rewrite(
        0,
        0,
        Some("Été, cœur et français corrigé"),
        &BlockStyle::default(),
    )
    .unwrap();
    let e = reopen(&mut e, "09-police-incorporee-reeditee.pdf");
    assert_eq!(e.blocks(0).unwrap()[0].base_font, font);
    assert!(plain(&e).contains("corrigé"));
    assert!(e.doc.scan_objects().any(|(_,obj)|matches!(obj.as_ref(),Object::Stream(s) if s.decoded().is_ok_and(|bytes|bytes==program))));
}
#[test]
fn rotation_crop_and_userunit_mapping_roundtrip() {
    for rotation in [0, 90, 180, 270] {
        let e = source();
        let mut d = e.doc.clone();
        let page = pdfcraft_model::pages(&d)[0].obj;
        d.update_dict(page, |p| {
            p.set(b"Rotate".to_vec(), Object::Int(rotation));
            p.set(b"UserUnit".to_vec(), Object::Real(1.5));
        })
        .unwrap();
        let mut e = Editor::from_bytes(serialize(&d)).unwrap();
        let p = e.info.pages[0].clone();
        for zoom in [0.5, 1.0, 1.75, 3.0] {
            let screen = [75.0 * zoom + 30.0, 120.0 * zoom - 50.0];
            let v = [(screen[0] - 30.0) / zoom, (screen[1] + 50.0) / zoom];
            let u = p.view_to_user(v[0], v[1]);
            let back = p.user_to_view(u[0], u[1]);
            assert!((back[0] - 75.0).abs() < 0.01 && (back[1] - 120.0).abs() < 0.01);
        }
        let a = p.view_to_user(0.0, 0.0);
        let b = p.view_to_user(25.0, 30.0);
        e.move_block(0, 0, [(b[0] - a[0]) as f64, (b[1] - a[1]) as f64])
            .unwrap();
        let e = reopen(&mut e, &format!("10-rotation-{rotation}.pdf"));
        assert_eq!(plain(&e).matches("originale").count(), 1);
    }
}
#[test]
fn failed_save_and_external_modification_preserve_original() {
    let dir = tempfile::tempdir_in(output("temp").parent().unwrap()).unwrap();
    let path = dir.path().join("original.pdf");
    let mut e = source();
    e.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    e.rewrite(0, 0, Some("Modifié"), &BlockStyle::default())
        .unwrap();
    assert!(e.save(&dir.path().join("absent/file.pdf")).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(e.dirty());
    std::fs::write(&path, b"external").unwrap();
    assert!(e.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"external");
}
#[test]
fn signed_document_needs_explicit_consent() {
    let mut d = source().doc;
    let mut sig = pdfcraft_cos::Dict::new();
    sig.set(b"Type".to_vec(), Object::name("Sig"));
    sig.set(
        b"ByteRange".to_vec(),
        Object::Array(vec![0.into(), 1.into(), 2.into(), 3.into()]),
    );
    let sig = d.add(sig);
    d.trailer_mut()
        .set(b"TestSignature".to_vec(), Object::Ref(sig));
    let mut e = Editor::from_bytes(serialize(&d)).unwrap();
    assert!(e.signed);
    assert!(
        e.rewrite(0, 0, Some("Refus"), &BlockStyle::default())
            .is_err()
    );
    e.signature_accepted = true;
    e.rewrite(0, 0, Some("Accepté explicitement"), &BlockStyle::default())
        .unwrap();
}

#[test]
fn organize_pages_keeps_content_images_annotations_and_history_after_reopen() {
    let mut d = pdfcraft_create::blank(595., 842., 3).unwrap();
    for page in 0..3 {
        pdfcraft_edit::add_content(
            &mut d,
            page,
            &Content::Text(AddedText {
                rect: [50., 700., 500., 740.],
                text: format!("Identité de la page {}", page + 1),
                ..Default::default()
            }),
        )
        .unwrap();
    }
    let mut e = Editor::from_bytes(serialize(&d)).unwrap();
    e.image_bytes(0, [100., 100., 200., 200.], "image.png", &png(), None)
        .unwrap();
    let text = e.text(0).unwrap();
    let range = text.find("Identité")[0].clone();
    e.highlight(0, &text, range).unwrap();
    let original = e.bytes.clone();
    e.rotate_page(0, 90).unwrap();
    assert!((e.info.pages[0].width - 842.).abs() < 0.1);
    e.move_page(0, 2).unwrap();
    for (page, identity) in [2, 3, 1].into_iter().enumerate() {
        assert!(
            e.text(page)
                .unwrap()
                .plain_text()
                .contains(&format!("page {identity}"))
        );
    }
    assert!((e.info.pages[2].width - 842.).abs() < 0.1);
    assert!(e.info.annotations.iter().any(|a| a.page == 2));
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.bytes, original);
    e.redo().unwrap();
    e.redo().unwrap();
    let mut e = reopen(&mut e, "11-pages-organisees.pdf");
    assert_eq!(e.info.pages.len(), 3);
    assert!(e.text(2).unwrap().plain_text().contains("page 1"));
    assert!(e.info.annotations.iter().any(|a| a.page == 2));
    let images = pdfcraft_edit::page_images(&e.doc, 2).unwrap();
    assert!(!images.is_empty());
    // Selection follows the actual PDF operators after a rotation, not old
    // display-space boxes in added-item metadata.
    assert!(e.added(2).is_empty());
    let mut renderer = PageRenderer::new(e.bytes.clone(), RenderConfig::default());
    for page in 0..3 {
        let result = renderer.render(RenderRequest {
            page,
            scale: 0.5,
            ..Default::default()
        });
        assert!(result.error.is_none(), "{:?}", result.error);
    }
    // Existing editing remains available after rotation, reordering and reopening.
    let text_index = e
        .blocks(2)
        .unwrap()
        .iter()
        .position(|a| a.editor_text.contains("Identité"))
        .unwrap();
    let before = e.blocks(2).unwrap()[text_index].rect;
    e.rewrite(
        2,
        text_index,
        Some("Page déplacée puis corrigée"),
        &BlockStyle::default(),
    )
    .unwrap();
    let e = reopen(&mut e, "11-pages-reeditees.pdf");
    assert!(e.text(2).unwrap().plain_text().contains("corrigée"));
    let after = e.blocks(2).unwrap()[0].rect;
    assert!((before[0] - after[0]).abs() < 0.1 && (before[3] - after[3]).abs() < 0.1);
}

#[test]
fn invalid_page_operations_are_atomic_and_no_op_does_not_create_history() {
    let mut e = source();
    let before = e.bytes.clone();
    assert!(e.rotate_page(0, 45).is_err());
    assert!(e.rotate_page(1, 90).is_err());
    assert!(e.move_page(0, 1).is_err());
    e.move_page(0, 0).unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    assert!(!e.dirty());
}
