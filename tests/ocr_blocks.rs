//! Spatial conversion contract with known recognition boxes. Native OCR itself
//! is exercised separately by the Windows UI test, without a recognition mock.
use pdfcraft_cos::{Object, SaveOptions};
use pdfcraft_edit::{AddedText, Content};
use pdfinator::{
    document::Editor,
    ocr::{Block, ImageSource, encode_png, erase_letters},
};

fn fixture() -> (Vec<u8>, Vec<u8>) {
    let mut pixels = vec![255; 400 * 220 * 4];
    // A diagram in a part of the raster that does not contain lettering.
    for x in 60..340 {
        let y = 80 + (x - 60) / 4;
        let at = (y * 400 + x) * 4;
        pixels[at..at + 3].fill(0);
    }
    for y in [80, 170] {
        for x in 60..340 {
            let at = (y * 400 + x) * 4;
            pixels[at..at + 3].fill(0);
        }
    }
    let boxes = [
        [20., 20., 40., 45.],
        [340., 20., 365., 45.],
        [20., 180., 100., 210.],
        [260., 180., 380., 210.],
    ];
    for r in boxes {
        for y in r[1] as usize..r[3] as usize {
            for x in r[0] as usize + 5..r[0] as usize + 9 {
                let at = (y * 400 + x) * 4;
                pixels[at..at + 3].fill(0);
            }
        }
    }
    let mut blocks = boxes
        .into_iter()
        .map(|rect| Block {
            text: "label".into(),
            rect,
            size: 14.,
            color: [0.; 3],
            words: vec![rect],
        })
        .collect::<Vec<_>>();
    let clean = erase_letters(&pixels, 400, 220, &mut blocks).unwrap();
    assert_eq!(
        &pixels[80 * 400 * 4..171 * 400 * 4],
        &clean[80 * 400 * 4..171 * 400 * 4]
    );
    (
        encode_png(&pixels, 400, 220).unwrap(),
        encode_png(&clean, 400, 220).unwrap(),
    )
}

#[test]
fn image_becomes_separate_positioned_text_blocks_and_a_clean_diagram_at_every_rotation() {
    let (picture, background) = fixture();
    for rotation in [0, 90, 180, 270] {
        for original in [false, true] {
            let mut doc = pdfcraft_create::blank(595., 842., 2).unwrap();
            let page = pdfcraft_model::pages(&doc)[0].obj;
            doc.update_dict(page, |d| {
                d.set(b"Rotate".to_vec(), Object::Int(rotation));
            })
            .unwrap();
            let mut e = Editor::from_bytes(
                pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(),
            )
            .unwrap();
            e.image_bytes(0, [60., 160., 460., 380.], "diagram.png", &picture, None)
                .unwrap();
            e.add_comment(0, [20., 20.], "Note à conserver").unwrap();
            e.add_text(
                1,
                AddedText {
                    rect: [40., 700., 500., 740.],
                    text: "Autre page intacte".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            if original {
                let item = pdfcraft_edit::list_added(&e.doc)
                    .into_iter()
                    .find(|a| a.page == 0)
                    .unwrap()
                    .obj;
                e.doc
                    .update_dict(item, |d| {
                        d.remove(b"PCMark");
                        d.remove(b"PCAdded");
                    })
                    .unwrap();
            }
            let mut e = Editor::from_bytes(
                pdfcraft_cos::write_full(&e.doc, &SaveOptions::default()).unwrap(),
            )
            .unwrap();
            let before = e.bytes.clone();
            let texts = vec![
                AddedText {
                    rect: [80., 335., 120., 365.],
                    text: "A".into(),
                    size: 14.,
                    ..Default::default()
                },
                AddedText {
                    rect: [400., 335., 440., 365.],
                    text: "B".into(),
                    size: 14.,
                    ..Default::default()
                },
                AddedText {
                    rect: [80., 170., 210., 200.],
                    text: "5 cm".into(),
                    size: 14.,
                    ..Default::default()
                },
                AddedText {
                    rect: [320., 170., 455., 200.],
                    text: "Été : œ, l’été".into(),
                    size: 14.,
                    ..Default::default()
                },
            ];
            let source = if original {
                ImageSource::Original(0)
            } else {
                ImageSource::Added(0)
            };
            let indices = e
                .image_to_blocks(
                    0,
                    source,
                    &background,
                    [60., 160., 460., 380.],
                    texts.clone(),
                )
                .unwrap();
            assert_eq!(indices.len(), 4);
            assert_eq!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().len(), 1);
            let converted = e.bytes.clone();
            e.undo().unwrap();
            assert_eq!(e.bytes, before);
            assert!(!e.can_undo());
            e.redo().unwrap();
            assert_eq!(e.bytes, converted);
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "test-output/23-ocr-regions-{}-{rotation}.pdf",
                if original { "original" } else { "ajout" }
            ));
            e.save(&path).unwrap();
            let mut reopened = Editor::open(&path).unwrap();
            assert_eq!(reopened.info.pages.len(), 2);
            assert_eq!(reopened.info.annotations.len(), 1);
            let added = reopened.added(0);
            assert_eq!(added.len(), 4);
            assert_eq!(
                pdfcraft_edit::page_images(&reopened.doc, 0).unwrap().len(),
                1,
                "The cleaned raster stays at its original drawing position"
            );
            for (index, expected) in indices.iter().zip(&texts) {
                let Content::Text(actual) = &added[*index] else {
                    panic!("Not editable PDF text");
                };
                assert_eq!(actual.text, expected.text);
                // The engine tightens the lower edge to the text's line height;
                // its left/right bounds and top anchor must remain at the source box.
                assert_eq!(actual.rect[0], expected.rect[0]);
                assert_eq!(actual.rect[2], expected.rect[2]);
                assert_eq!(actual.rect[3], expected.rect[3]);
                assert_eq!(actual.size, expected.size);
                assert_eq!(
                    reopened
                        .text(0)
                        .unwrap()
                        .plain_text()
                        .matches(&expected.text)
                        .count(),
                    1
                );
            }
            assert!(
                reopened
                    .text(1)
                    .unwrap()
                    .plain_text()
                    .contains("Autre page intacte")
            );
            let mut corrected = texts[2].clone();
            corrected.text = "6 cm".into();
            reopened
                .update_added(0, indices[2], Content::Text(corrected))
                .unwrap();
            let text = reopened.text(0).unwrap().plain_text();
            assert!(text.contains("6 cm"));
            assert!(!text.contains("5 cm"));
        }
    }
}

#[test]
fn invalid_block_or_background_leaves_the_entire_conversion_unapplied() {
    let (picture, background) = fixture();
    let doc = pdfcraft_create::blank(595., 842., 1).unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    e.image_bytes(0, [60., 160., 460., 380.], "diagram.png", &picture, None)
        .unwrap();
    let before = e.bytes.clone();
    for (png, text) in [
        (background.as_slice(), "\u{10ffff}"),
        (b"invalid PNG".as_slice(), "Texte"),
    ] {
        assert!(
            e.image_to_blocks(
                0,
                ImageSource::Added(0),
                png,
                [60., 160., 460., 380.],
                vec![AddedText {
                    text: text.into(),
                    rect: [80., 180., 300., 220.],
                    ..Default::default()
                }]
            )
            .is_err()
        );
        assert_eq!(e.bytes, before);
        assert_eq!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().len(), 1);
    }
}

#[test]
fn an_old_invisible_ocr_layer_is_replaced_without_duplicate_text_and_undo_restores_it() {
    let (picture, background) = fixture();
    let d = pdfcraft_create::from_text("", "Old OCR", (595., 842.), 14.).unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
    let b = e.blocks(0).unwrap().remove(0);
    e.move_block(0, 0, [80. - b.rect[0], 300. - b.rect[1]])
        .unwrap();
    let page = pdfcraft_model::pages(&e.doc).remove(0);
    let old = page.dict.get(b"Contents").unwrap().clone();
    let prefix = e.doc.add(Object::Stream(pdfcraft_cos::Stream::flate(
        pdfcraft_cos::Dict::new(),
        b"q 3 Tr\n",
    )));
    let suffix = e.doc.add(Object::Stream(pdfcraft_cos::Stream::flate(
        pdfcraft_cos::Dict::new(),
        b"Q\n",
    )));
    let mut list = vec![Object::Ref(prefix)];
    match &*e.doc.resolve(&old) {
        Object::Array(items) => list.extend(items.clone()),
        _ => list.push(old),
    }
    list.push(Object::Ref(suffix));
    e.doc
        .update_dict(page.obj, |d| {
            d.set(b"Contents".to_vec(), Object::Array(list))
        })
        .unwrap();
    e = Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap();
    assert!(e.blocks(0).unwrap()[0].invisible);
    e.image_bytes(0, [60., 160., 460., 380.], "scan.png", &picture, None)
        .unwrap();
    let before = e.bytes.clone();
    let (_, area) = e.image_for_ocr(0, ImageSource::Added(0)).unwrap();
    assert!(e.image_ocr_retained_text(0, area).unwrap().is_empty());
    e.image_to_blocks(
        0,
        ImageSource::Added(0),
        &background,
        [60., 160., 460., 380.],
        vec![AddedText {
            text: "Real editable text".into(),
            rect: [80., 300., 330., 330.],
            size: 14.,
            ..Default::default()
        }],
    )
    .unwrap();
    let text = e.text(0).unwrap().plain_text();
    assert!(!text.contains("Old OCR"), "{text}");
    assert_eq!(text.matches("Real editable text").count(), 1);
    assert!(e.blocks(0).unwrap().iter().all(|b| !b.invisible));
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}
