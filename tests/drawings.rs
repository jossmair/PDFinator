use pdfcraft_cos::{Dict, Object, Stream};
use pdfinator::{
    document::Editor,
    drawing::{Drawing, Kind, StrokeStyle, Target},
};
fn editor(rotation: u16) -> Editor {
    let mut d =
        pdfcraft_create::from_text("Traits", "Le texte reste intact.", (595., 842.), 14.).unwrap();
    let p = pdfcraft_model::pages(&d)[0].obj;
    d.update_dict(p, |p| {
        p.set(b"Rotate".to_vec(), Object::Int(rotation as i64))
    })
    .unwrap();
    Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap()
}
fn output(e: &mut Editor, name: &str) -> Editor {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output")
        .join(name);
    e.save(&p).unwrap();
    Editor::open(&p).unwrap()
}
#[test]
fn an_explicitly_closed_rectangular_clip_does_not_hide_filled_fraction_bars() {
    let mut e = editor(0);
    let page = pdfcraft_model::pages(&e.doc).remove(0);
    let old = page.dict.get(b"Contents").unwrap().clone();
    let raw=b"q 200 300 m 100 300 l 100 400 l 200 400 l 200 300 l h W* n 0.22745 0.22352 0.20784 rg 120 350 50 0.8 re f* Q";
    let stream = e.doc.add(Object::Stream(Stream::flate(Dict::new(), raw)));
    e.doc
        .update_dict(page.obj, |d| {
            d.set(
                b"Contents".to_vec(),
                Object::Array(vec![old, Object::Ref(stream)]),
            )
        })
        .unwrap();
    let e =
        Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap();
    let items = e.drawings(0).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].drawing.kind, Kind::Line);
    assert!((items[0].drawing.width - 0.8).abs() < 1e-7);
    assert!((items[0].drawing.u[0] - 50.).abs() < 1e-7);
}
#[test]
fn rectangular_writer_clipping_keeps_visible_strokes_editable_and_edits_inside_the_clip() {
    let mut d =
        pdfcraft_create::from_text("Découpe", "Texte à conserver.", (595., 842.), 14.).unwrap();
    let p = pdfcraft_model::pages(&d)[0].clone();
    let old = p.dict.get(b"Contents").unwrap().clone();
    let stream = d.add(Object::Stream(Stream::flate(Dict::new(),
        b"q 0 0 595 842 re W* n 0 G 1 w 293.5 677.8 m 293.5 827.2 l 581 827.2 l 581 677.8 l h S q 300 700 250 100 re W n 310 740 m 540 740 l S Q Q")));
    d.update_dict(p.obj, |p| {
        p.set(
            b"Contents".to_vec(),
            Object::Array(vec![old, Object::Ref(stream)]),
        )
    })
    .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
    let items = e.drawings(0).unwrap();
    assert_eq!(items.len(), 5);
    let before = e.bytes.clone();
    let item = items
        .iter()
        .find(|i| (i.drawing.a[0] - 310.).abs() < 0.01)
        .unwrap();
    let mut updated = item.drawing.clone();
    updated.translate([0., -10.]);
    updated.width = 3.;
    e.change_drawings(0, &[(item.target, Some(updated.clone()))])
        .unwrap();
    let reopened = output(&mut e, "27-decoupe-writer.pdf");
    assert_eq!(reopened.drawings(0).unwrap().len(), 5);
    assert!(
        reopened
            .drawings(0)
            .unwrap()
            .iter()
            .any(|i| i.drawing == updated)
    );
    assert!(reopened.text(0).unwrap().plain_text().contains("conserver"));
    let mut outside = updated.clone();
    outside.translate([0., 80.]);
    let item = e
        .drawings(0)
        .unwrap()
        .into_iter()
        .find(|i| i.drawing == updated)
        .unwrap();
    let safe = e.bytes.clone();
    e.change_drawings(0, &[(item.target, Some(outside.clone()))])
        .unwrap();
    assert!(e.drawings(0).unwrap().iter().any(|i| i.drawing == outside));
    let reopened = output(&mut e, "46-trait-hors-cadre.pdf");
    assert!(
        reopened
            .drawings(0)
            .unwrap()
            .iter()
            .any(|i| i.drawing == outside)
    );
    e.undo().unwrap();
    assert_eq!(e.bytes, safe);
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}

#[test]
fn writer_oblique_filled_strokes_move_resize_and_restyle_outside_their_original_clip() {
    let mut e = if let Some(path) = std::env::var_os("PDFINATOR_OBLIQUE_PDF") {
        Editor::open(std::path::Path::new(&path)).unwrap()
    } else {
        let mut e = editor(0);
        let p = pdfcraft_model::pages(&e.doc).remove(0);
        let old = p.dict.get(b"Contents").unwrap().clone();
        let raw=b"q 20 400 160 150 re W n 1 1 1 rg 21 401 158 148 re f 0 g 40 449.7 m 140 499.7 l 140.3 500.3 l 40.3 450.3 l h f 40 509.7 m 140 459.7 l 140.3 460.3 l 40.3 510.3 l h f Q";
        let s = e.doc.add(Object::Stream(Stream::flate(Dict::new(), raw)));
        e.doc
            .update_dict(p.obj, |d| {
                d.set(
                    b"Contents".to_vec(),
                    Object::Array(vec![old, Object::Ref(s)]),
                )
            })
            .unwrap();
        Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap()
    };
    let before = e.bytes.clone();
    let native = e.text(0).unwrap().plain_text();
    let all = e.drawings(0).unwrap();
    let diagonal = all
        .iter()
        .filter(|i| {
            i.drawing.kind == Kind::Line && i.drawing.u[0].abs() > 20. && i.drawing.u[1].abs() > 20.
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        diagonal.len() >= 2,
        "Oblique filled strokes must be offered as lines: {all:?}"
    );
    for (n, item) in diagonal.iter().take(4).enumerate() {
        let mut changed = item.drawing.clone();
        let direction = if changed.a[0] > 400. { -1. } else { 1. };
        changed.translate([direction * 100., -100.]);
        changed.u = changed.u.map(|v| v * 1.3);
        changed.width = 3.5;
        changed.color = [0.8, 0.1, 0.2];
        changed.style = StrokeStyle::Dashed;
        let matches = |d: &Drawing| {
            d.kind == changed.kind
                && d.style == changed.style
                && d.color == changed.color
                && (d.width - changed.width).abs() < 1e-8
                && d.a
                    .iter()
                    .chain(&d.u)
                    .chain(&d.v)
                    .zip(changed.a.iter().chain(&changed.u).chain(&changed.v))
                    .all(|(a, b)| (a - b).abs() < 1e-7)
        };
        e.change_drawings(0, &[(item.target, Some(changed.clone()))])
            .unwrap();
        assert!(
            e.drawings(0).unwrap().iter().any(|i| matches(&i.drawing)),
            "case {n}, desired {changed:?}, actual {:?}",
            e.drawings(0)
                .unwrap()
                .iter()
                .filter(|i| (i.drawing.width - 3.5).abs() < 0.01)
                .collect::<Vec<_>>()
        );
        assert_eq!(e.drawings(0).unwrap().len(), all.len());
        assert_eq!(e.text(0).unwrap().plain_text(), native);
        let prefix = if std::env::var_os("PDFINATOR_OBLIQUE_PDF").is_some() {
            "46-figuresexo"
        } else {
            "46-polygones"
        };
        let reopened = output(&mut e, &format!("{prefix}-{n}.pdf"));
        std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("test-output/{prefix}-{n}.json")),
            serde_json::to_vec_pretty(&serde_json::json!({"old":item.drawing,"new":changed,"page_height":e.info.pages[0].height})).unwrap()).unwrap();
        assert!(
            reopened
                .drawings(0)
                .unwrap()
                .iter()
                .any(|i| matches(&i.drawing))
        );
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
    }
}
#[test]
fn original_table_strokes_edit_individually_then_move_together_without_duplicates() {
    let mut e = editor(0);
    let p = pdfcraft_model::pages(&e.doc)[0].clone();
    let line=e.doc.add(Object::Stream(Stream::flate(Dict::new(),b"q 0.5 w 0 G 30 500 m 550 500 l 30 460 m 550 460 l 30 420 m 550 420 l 30 420 m 30 500 l 250 420 m 250 500 l 550 420 m 550 500 l S Q")));
    let old = p.dict.get(b"Contents").unwrap().clone();
    e.doc
        .update_dict(p.obj, |d| {
            d.set(
                b"Contents".to_vec(),
                Object::Array(vec![old, Object::Ref(line)]),
            )
        })
        .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_incremental(&e.doc, &Default::default()).unwrap())
            .unwrap();
    let before = e.bytes.clone();
    let items = e.drawings(0).unwrap();
    assert_eq!(items.len(), 6);
    let mut updated = items[0].drawing.clone();
    updated.u[0] -= 50.;
    updated.width = 3.;
    updated.color = [0.8, 0.1, 0.2];
    e.change_drawings(0, &[(items[0].target, Some(updated.clone()))])
        .unwrap();
    let after = e.drawings(0).unwrap();
    assert_eq!(after.len(), 6);
    assert!(after.iter().any(|i| i.drawing == updated));
    assert_eq!(
        after
            .iter()
            .filter(|i| (i.drawing.width - 0.5).abs() < 0.01)
            .count(),
        5
    );
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
    e.redo().unwrap();
    let moved = e
        .drawings(0)
        .unwrap()
        .iter()
        .map(|i| {
            let mut d = i.drawing.clone();
            d.translate([12., -18.]);
            (i.target, Some(d))
        })
        .collect::<Vec<_>>();
    e.change_drawings(0, &moved).unwrap();
    let reopened = output(&mut e, "22-traits-tableau.pdf");
    assert_eq!(reopened.drawings(0).unwrap().len(), 6);
    assert_eq!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .matches("reste intact")
            .count(),
        1
    );
    assert!(
        reopened
            .drawings(0)
            .unwrap()
            .iter()
            .any(|i| (i.drawing.a[0] - 42.).abs() < 0.01 && (i.drawing.a[1] - 482.).abs() < 0.01)
    );
}
#[test]
fn shapes_are_native_editable_vectors_at_all_rotations_and_owned_edits_persist() {
    for rotation in [0, 90, 180, 270] {
        let mut e = editor(rotation);
        let p = e.info.pages[0].clone();
        let before = e.bytes.clone();
        let mut targets = Vec::new();
        for (n, kind) in [Kind::Line, Kind::Circle, Kind::Rectangle, Kind::Table]
            .into_iter()
            .enumerate()
        {
            let a = [60. + n as f32 * 90., 300.];
            let b = [a[0] + 75., 380.];
            let d = Drawing::from_view(&p, kind, a, b, 1.5, [0.1, 0.3, 0.7], 3, 4);
            let r = e.add_drawing(0, d).unwrap();
            targets.push(r);
        }
        let items = e.drawings(0).unwrap();
        assert_eq!(items.len(), 4);
        let mut d = items
            .iter()
            .find(|i| i.target == Target::Owned(targets[2]))
            .unwrap()
            .drawing
            .clone();
        d.width *= 2.;
        d.translate([10., 12.]);
        e.change_drawings(0, &[(Target::Owned(targets[2]), Some(d.clone()))])
            .unwrap();
        assert!(e.drawings(0).unwrap().iter().any(|i| i.drawing == d));
        let mut reopened = output(&mut e, &format!("22-formes-{rotation}.pdf"));
        assert_eq!(reopened.drawings(0).unwrap().len(), 4);
        assert!(reopened.drawings(0).unwrap().iter().any(|i| i.drawing == d));
        assert!(
            pdfcraft_edit::page_images(&reopened.doc, 0)
                .unwrap()
                .is_empty()
        );
        reopened
            .change_drawings(0, &[(Target::Owned(targets[1]), None)])
            .unwrap();
        assert_eq!(reopened.drawings(0).unwrap().len(), 3);
        e.undo().unwrap();
        for _ in 0..4 {
            e.undo().unwrap();
        }
        assert_eq!(e.bytes, before);
    }
}
#[test]
fn unsafe_or_invalid_path_changes_are_atomic_and_clipped_curves_are_not_offered() {
    let mut e = editor(0);
    let before = e.bytes.clone();
    let p = e.info.pages[0].clone();
    let mut d = Drawing::from_view(
        &p,
        Kind::Rectangle,
        [60., 300.],
        [200., 400.],
        1.,
        [0.; 3],
        1,
        1,
    );
    d.width = f64::NAN;
    assert!(e.add_drawing(0, d).is_err());
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    let p = pdfcraft_model::pages(&e.doc)[0].obj;
    let stream = e.doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 0 0 400 500 re W n 10 10 m 500 10 l S Q q 30 40 m 80 100 100 180 200 200 c S Q",
    )));
    e.doc
        .update_dict(p, |p| {
            p.set(b"Contents".to_vec(), Object::Ref(stream));
        })
        .unwrap();
    let e =
        Editor::from_bytes(pdfcraft_cos::write_incremental(&e.doc, &Default::default()).unwrap())
            .unwrap();
    assert!(
        e.drawings(0).unwrap().is_empty(),
        "Clipped or curved source paths must not be offered as straight strokes"
    );
}

#[test]
fn owned_table_can_be_split_and_each_stroke_edited_with_one_undo_per_action() {
    let mut e = editor(90);
    let p = e.info.pages[0].clone();
    let target = e
        .add_drawing(
            0,
            Drawing::from_view(
                &p,
                Kind::Table,
                [80., 140.],
                [400., 350.],
                1.5,
                [0.; 3],
                3,
                4,
            ),
        )
        .unwrap();
    let table_bytes = e.bytes.clone();
    let targets = e.split_drawing(0, Target::Owned(target)).unwrap();
    assert_eq!(targets.len(), 9);
    let items = e.drawings(0).unwrap();
    assert_eq!(items.len(), 9);
    assert!(items.iter().all(|i| i.drawing.kind == Kind::Line));
    e.undo().unwrap();
    assert_eq!(e.bytes, table_bytes);
    e.redo().unwrap();
    let mut line = items[0].drawing.clone();
    line.u[0] *= 0.75;
    line.u[1] *= 0.75;
    line.width = 4.;
    line.color = [0.8, 0.2, 0.1];
    e.change_drawings(0, &[(items[0].target, Some(line.clone()))])
        .unwrap();
    let e = output(&mut e, "22-tableau-separe.pdf");
    assert_eq!(e.drawings(0).unwrap().len(), 9);
    assert!(e.drawings(0).unwrap().iter().any(|i| i.drawing == line));
}

#[test]
fn device_rgb_and_extgstate_strokes_keep_their_transformation_and_neighbours() {
    let mut e = editor(0);
    let p = pdfcraft_model::pages(&e.doc)[0].obj;
    let mut gs = Dict::new();
    gs.set(b"LW".to_vec(), Object::Real(0.75));
    let mut states = Dict::new();
    states.set(b"G1".to_vec(), Object::Dict(gs));
    let old = e
        .doc
        .dict(&Object::Ref(p))
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    let mut resources = e.doc.dict(&old).unwrap().clone();
    resources.set(b"ExtGState".to_vec(), Object::Dict(states));
    let content = e.doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 2 0 0 2 10 30 cm /G1 gs /DeviceRGB CS 0.2 0.4 0.6 SCN 10 50 m 100 50 l S Q",
    )));
    let old_content = e
        .doc
        .dict(&Object::Ref(p))
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    e.doc
        .update_dict(p, |d| {
            d.set(
                b"Contents".to_vec(),
                Object::Array(vec![old_content, Object::Ref(content)]),
            );
            d.set(b"Resources".to_vec(), Object::Dict(resources));
        })
        .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_incremental(&e.doc, &Default::default()).unwrap())
            .unwrap();
    let items = e.drawings(0).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].drawing.a, [30., 130.]);
    assert_eq!(items[0].drawing.width, 1.5);
    let mut moved = items[0].drawing.clone();
    moved.translate([15., -12.]);
    e.change_drawings(0, &[(items[0].target, Some(moved.clone()))])
        .unwrap();
    let e = output(&mut e, "22-trait-transforme.pdf");
    assert_eq!(e.drawings(0).unwrap().len(), 1);
    assert_eq!(e.drawings(0).unwrap()[0].drawing, moved);
    assert!(e.text(0).unwrap().plain_text().contains("reste intact"));
}

#[test]
fn preset_strokes_persist_as_native_pdf_styles_at_every_rotation() {
    let styles = [
        StrokeStyle::Solid,
        StrokeStyle::Dashed,
        StrokeStyle::Dotted,
        StrokeStyle::DashDot,
        StrokeStyle::LongDash,
        StrokeStyle::Double,
    ];
    for rotation in [0, 90, 180, 270] {
        let mut e = editor(rotation);
        let p = e.info.pages[0].clone();
        for (index, style) in styles.iter().enumerate() {
            let y = 150. + index as f32 * 65.;
            e.add_text(
                0,
                pdfcraft_edit::AddedText {
                    rect: [
                        40.,
                        (p.height - y - 18.) as f64,
                        190.,
                        (p.height - y + 8.) as f64,
                    ],
                    text: style.label().into(),
                    size: 12.,
                    ..Default::default()
                },
            )
            .unwrap();
            let d = Drawing::from_view(
                &p,
                Kind::Line,
                [220., y],
                [510., y],
                3.,
                [0.13, 0.33, 0.95],
                1,
                1,
            )
            .with_style(style.clone());
            e.add_drawing(0, d).unwrap();
        }
        let mut reopened = output(&mut e, &format!("24-styles-{rotation}.pdf"));
        let items = reopened.drawings(0).unwrap();
        assert_eq!(items.len(), 6);
        assert_eq!(
            items
                .iter()
                .map(|i| i.drawing.style.clone())
                .collect::<Vec<_>>(),
            styles
        );
        let double = &items[5];
        let mut changed = double.drawing.clone();
        changed.u = changed.u.map(|v| v * 0.8);
        changed.translate([10., 12.]);
        reopened
            .change_drawings(0, &[(double.target, Some(changed.clone()))])
            .unwrap();
        let persisted = Editor::from_bytes((*reopened.bytes).clone()).unwrap();
        assert_eq!(persisted.drawings(0).unwrap().len(), 6);
        assert!(
            persisted
                .drawings(0)
                .unwrap()
                .iter()
                .any(|i| i.drawing == changed)
        );
    }
}

#[test]
fn source_dash_phase_and_caps_survive_editing_under_a_scaled_matrix() {
    let mut e = editor(0);
    let p = pdfcraft_model::pages(&e.doc)[0].obj;
    let old = e
        .doc
        .dict(&Object::Ref(p))
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    let stream = e.doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 2 0 0 2 10 30 cm 1 w 1 J [4 2 1 2] 1 d 30 100 m 240 100 l S Q",
    )));
    e.doc
        .update_dict(p, |p| {
            p.set(
                b"Contents".to_vec(),
                Object::Array(vec![old, Object::Ref(stream)]),
            );
        })
        .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_incremental(&e.doc, &Default::default()).unwrap())
            .unwrap();
    let item = e.drawings(0).unwrap().remove(0);
    assert_eq!(
        item.drawing.style,
        StrokeStyle::Custom {
            pattern: vec![8., 4., 2., 4.],
            phase: 2.,
            cap: 1
        }
    );
    let before = e.bytes.clone();
    let mut d = item.drawing.clone();
    d.width = 4.5;
    d.translate([15., -20.]);
    e.change_drawings(0, &[(item.target, Some(d.clone()))])
        .unwrap();
    let reopened = output(&mut e, "24-style-origine.pdf");
    assert_eq!(reopened.drawings(0).unwrap().len(), 1);
    assert_eq!(reopened.drawings(0).unwrap()[0].drawing, d);
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}

#[test]
fn drawings_without_style_metadata_keep_the_legacy_solid_style_and_invalid_dashes_are_atomic() {
    let mut e = editor(0);
    let p = e.info.pages[0].clone();
    let drawing = Drawing::from_view(
        &p,
        Kind::Line,
        [60., 300.],
        [300., 300.],
        1.5,
        [0.; 3],
        1,
        1,
    );
    let mut json = serde_json::to_value(&drawing).unwrap();
    json.as_object_mut().unwrap().remove("style");
    let legacy: Drawing = serde_json::from_value(json).unwrap();
    assert_eq!(legacy, drawing);
    let before = e.bytes.clone();
    for pattern in [vec![0., 0.], vec![-1., 2.], vec![f64::INFINITY, 1.]] {
        let d = drawing.clone().with_style(StrokeStyle::Custom {
            pattern,
            phase: 0.,
            cap: 1,
        });
        assert!(e.add_drawing(0, d).is_err());
        assert_eq!(e.bytes, before);
        assert!(!e.can_undo());
    }
}
