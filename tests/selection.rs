use pdfcraft_cos::{Dict, Object, Stream};
use pdfinator::{document::Editor, selection::Target};
fn fixture(rotation: i64) -> Editor {
    let mut doc =
        pdfcraft_create::from_text("Sélection", "Texte à déplacer ensemble", (595., 842.), 16.)
            .unwrap();
    let p = pdfcraft_model::pages(&doc)[0].clone();
    let old = p.dict.get(b"Contents").unwrap().clone();
    // A clipped curve, a native filled arrow and a multi-segment path. A visible
    // part can move outside its OLD clipping box, with its clipping shape intact.
    let s=doc.add(Object::Stream(Stream::flate(Dict::new(),b"q 200 300 160 160 re W n 0.1 0.3 0.7 RG 3 w 190 320 m 220 480 340 480 370 320 c S Q\n0.9 0.2 0.2 rg 100 200 m 120 190 l 100 180 l h f\n0 G 2 w 70 180 m 100 190 l 70 200 l S\n")));
    doc.update_dict(p.obj, |d| {
        d.set(
            b"Contents".to_vec(),
            Object::Array(vec![old, Object::Ref(s)]),
        );
        d.set(b"Rotate".to_vec(), Object::Int(rotation));
    })
    .unwrap();
    Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap()
}
#[test]
fn zero_width_pdf_hairlines_are_visible_selectable_and_movable() {
    let mut d = pdfcraft_create::blank(200., 200., 1).unwrap();
    let p = pdfcraft_model::pages(&d)[0].obj;
    let r = d.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"0 w 20 40 m 100 40 l S 80 50 m 80 120 l S",
    )));
    d.update_dict(p, |d| d.set(b"Contents".to_vec(), Object::Ref(r)))
        .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
    let items = e.selection_items(0).unwrap();
    assert_eq!(items.len(), 2);
    for i in &items {
        assert!(i.rect[2] > i.rect[0] && i.rect[3] > i.rect[1]);
    }
    e.move_selection(
        0,
        &items.iter().map(|i| i.target).collect::<Vec<_>>(),
        [10., -5.],
    )
    .unwrap();
    assert_eq!(e.selection_items(0).unwrap().len(), 2);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/29-traits-fins.pdf");
    e.save(&path).unwrap();
}
#[test]
fn curves_filled_arrows_text_and_images_move_as_one_native_transaction_at_every_rotation() {
    for rotation in [0, 90, 180, 270] {
        let mut e = fixture(rotation);
        let pixels = vec![70; 12 * 12 * 4];
        let png = pdfinator::ocr::encode_png(&pixels, 12, 12).unwrap();
        e.image_bytes(0, [400., 300., 450., 350.], "transparence.png", &png, None)
            .unwrap();
        let before = e.bytes.clone();
        let items = e.selection_items(0).unwrap();
        assert_eq!(
            items
                .iter()
                .filter(|i| matches!(i.target, Target::Paint(_)))
                .count(),
            3
        );
        let target = items.iter().map(|i| i.target).collect::<Vec<_>>();
        e.move_selection(0, &target, [40., -20.]).unwrap();
        let moved = e.selection_items(0).unwrap();
        assert_eq!(moved.len(), items.len(), "No parasite retained");
        for old in &items {
            assert!(
                moved.iter().any(|i| i
                    .rect
                    .iter()
                    .zip([
                        old.rect[0] + 40.,
                        old.rect[1] - 20.,
                        old.rect[2] + 40.,
                        old.rect[3] - 20.
                    ])
                    .all(|(a, b)| (*a - b).abs() < 0.05)),
                "Missing moved {:?}; got {:?}",
                old,
                moved
            );
        }
        assert_eq!(
            e.text(0)
                .unwrap()
                .plain_text()
                .matches("déplacer ensemble")
                .count(),
            1
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test-output/29-selection-mixte-{rotation}.pdf"));
        e.save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        assert_eq!(reopened.selection_items(0).unwrap().len(), items.len());
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
        e.redo().unwrap();
        assert_ne!(e.bytes, before);
        let all = e
            .selection_items(0)
            .unwrap()
            .iter()
            .map(|i| i.target)
            .collect::<Vec<_>>();
        e.delete_selection(0, &all).unwrap();
        assert!(e.selection_items(0).unwrap().is_empty());
        e.undo().unwrap();
        assert_eq!(e.selection_items(0).unwrap().len(), items.len());
    }
}
#[test]
fn extraction_preview_retains_only_selected_paints_without_modifying_the_document() {
    let e = fixture(0);
    let before = e.bytes.clone();
    let items = e.selection_items(0).unwrap();
    let targets = items
        .iter()
        .filter(|i| matches!(i.target, Target::Paint(_)))
        .map(|i| i.target)
        .collect::<Vec<_>>();
    let bytes = e.selection_preview(0, &targets, true, false).unwrap();
    let selected = Editor::from_bytes((*bytes).clone()).unwrap();
    assert!(selected.text(0).unwrap().plain_text().trim().is_empty());
    assert_eq!(selected.selection_items(0).unwrap().len(), 3);
    let bytes = e.selection_preview(0, &targets, false, false).unwrap();
    let background = Editor::from_bytes((*bytes).clone()).unwrap();
    assert!(
        background
            .selection_items(0)
            .unwrap()
            .iter()
            .all(|i| !matches!(i.target, Target::Paint(_)))
    );
    assert_eq!(e.bytes, before);
}

#[test]
fn rich_text_decorations_and_comments_follow_a_mixed_move_and_remain_editable() {
    use pdfinator::rich::{Format, Span, TextData};
    let mut e = fixture(0);
    let b = e.blocks(0).unwrap().remove(0);
    let data = TextData {
        id: "selection-rich".into(),
        text: b.text.clone(),
        rect: b.rect,
        base_size: 16.,
        base_color: [0.; 3],
        base_bold: false,
        base_italic: false,
        font_name: "Helvetica".into(),
        align: None,
        line_spacing: None,
        spans: vec![Span {
            range: 0..b.text.len(),
            format: Format {
                underline: Some(true),
                background: Some([1., 1., 0.]),
                ..Default::default()
            },
        }],
    };
    e.format_text(0, Some(0), None, data, &[]).unwrap();
    e.add_comment(0, [460., 700.], "Note à conserver").unwrap();
    let before = e.bytes.clone();
    let data = e.text_formatting(0, 0).unwrap();
    let items = e.selection_items(0).unwrap();
    // Underline/background are part of their text, not unrelated selectable paints.
    assert_eq!(
        items
            .iter()
            .filter(|i| matches!(i.target, Target::Paint(_)))
            .count(),
        3
    );
    assert!(
        items
            .iter()
            .any(|i| matches!(i.target, Target::Annotation(_)))
    );
    e.move_selection(
        0,
        &items.iter().map(|i| i.target).collect::<Vec<_>>(),
        [15., -12.],
    )
    .unwrap();
    let moved = e.text_formatting(0, 0).unwrap();
    assert_eq!(moved.text, data.text);
    assert_eq!(moved.spans, data.spans);
    for (a, b) in moved.rect.into_iter().zip([
        data.rect[0] + 15.,
        data.rect[1] - 12.,
        data.rect[2] + 15.,
        data.rect[3] - 12.,
    ]) {
        assert!((a - b).abs() < 0.001);
    }
    let note = e
        .info
        .annotations
        .iter()
        .find(|a| a.subtype == "Text")
        .unwrap();
    assert!(
        note.contents
            .as_deref()
            .unwrap_or_default()
            .contains("Note")
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output/29-selection-texte-style.pdf");
    e.save(&path).unwrap();
    let mut reopened = Editor::open(&path).unwrap();
    let mut next = reopened.text_formatting(0, 0).unwrap();
    next.text = "Toujours modifiable".into();
    next.spans.clear();
    reopened.format_text(0, Some(0), None, next, &[]).unwrap();
    assert!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .contains("Toujours modifiable")
    );
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}
#[test]
fn native_background_stays_behind_text_and_added_circle_remains_editable_after_group_move() {
    use pdfinator::drawing::{Drawing, Kind};
    let mut e = fixture(0);
    let b = e.blocks(0).unwrap().remove(0);
    let p = pdfcraft_model::pages(&e.doc)[0].clone();
    let old = p.dict.get(b"Contents").unwrap().clone();
    let raw = format!(
        "q 0.7 0.85 0.5 rg {} {} {} {} re f Q",
        b.rect[0] - 5.,
        b.rect[1] - 5.,
        b.rect[2] - b.rect[0] + 10.,
        b.rect[3] - b.rect[1] + 10.
    );
    let r = e
        .doc
        .add(Object::Stream(Stream::flate(Dict::new(), raw.as_bytes())));
    e.doc
        .update_dict(p.obj, |d| {
            d.set(
                b"Contents".to_vec(),
                Object::Array(vec![Object::Ref(r), old]),
            )
        })
        .unwrap();
    // The fixture's original Contents is an array; flatten it rather than creating
    // an invalid nested content array.
    let p = pdfcraft_model::pages(&e.doc)[0].clone();
    let values = p
        .dict
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|o| match o {
            Object::Array(a) => a.clone(),
            _ => vec![o.clone()],
        })
        .collect();
    e.doc
        .update_dict(p.obj, |d| {
            d.set(b"Contents".to_vec(), Object::Array(values))
        })
        .unwrap();
    e = Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap();
    let circle = Drawing::from_view(
        &e.info.pages[0],
        Kind::Circle,
        [380., 150.],
        [450., 230.],
        2.,
        [0.1, 0.2, 0.8],
        1,
        1,
    );
    e.add_drawing(0, circle.clone()).unwrap();
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
    e.save(&output.join("29-calques-original.pdf")).unwrap();
    let targets = e
        .selection_items(0)
        .unwrap()
        .into_iter()
        .map(|i| i.target)
        .collect::<Vec<_>>();
    e.move_selection(0, &targets, [15., -12.]).unwrap();
    let item = e
        .drawings(0)
        .unwrap()
        .into_iter()
        .find(|i| i.drawing.kind == Kind::Circle)
        .unwrap();
    assert_eq!(item.drawing.kind, Kind::Circle);
    assert_eq!(item.drawing.a, [circle.a[0] + 15., circle.a[1] - 12.]);
    e.save(&output.join("29-calques-deplaces.pdf")).unwrap();
    let n = e.selection_items(0).unwrap().len();
    let mut changed = item.drawing;
    changed.width = 4.;
    e.change_drawings(0, &[(item.target, Some(changed))])
        .unwrap();
    assert_eq!(
        e.selection_items(0).unwrap().len(),
        n,
        "Editing the moved circle must not leave a duplicate"
    );
}
