use pdfcraft_cos::{Dict, ObjRef, Object, Stream};
use pdfinator::{document::Editor, drawing::Kind};

fn fixture(body: &[u8], transparent: bool) -> (Editor, ObjRef) {
    fixture_matrix(body, transparent, [1.2, 0., 0., 1.2, 2., 3.])
}
fn fixture_matrix(body: &[u8], transparent: bool, matrix: [f64; 6]) -> (Editor, ObjRef) {
    let mut doc = pdfcraft_create::blank(420., 400., 2).unwrap();
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Ref(doc.add(font.clone())));
    let mut inner = Dict::new();
    inner.set(b"Font".to_vec(), Object::Dict(fonts));
    let mut form = Dict::new();
    form.set(b"Type".to_vec(), Object::name("XObject"));
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(
        b"BBox".to_vec(),
        Object::Array(vec![0.into(), 0.into(), 120.into(), 90.into()]),
    );
    form.set(
        b"Matrix".to_vec(),
        Object::Array(matrix.map(Object::Real).to_vec()),
    );
    form.set(b"Resources".to_vec(), Object::Dict(inner));
    if transparent {
        let mut group = Dict::new();
        group.set(b"S".to_vec(), Object::name("Transparency"));
        form.set(b"Group".to_vec(), Object::Dict(group));
    }
    let object = doc.add(Stream::flate(form, body));
    font.set(b"BaseFont".to_vec(), Object::name("Courier"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Ref(doc.add(font)));
    let mut xo = Dict::new();
    xo.set(b"Fm".to_vec(), Object::Ref(object));
    let mut resources = Dict::new();
    resources.set(b"XObject".to_vec(), Object::Dict(xo));
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    // Both pages share their resource dictionary and their Form object.
    let resources = doc.add(resources);
    for p in pdfcraft_model::pages(&doc) {
        let stream = doc.add(Stream::flate(Dict::new(), b"BT /F1 14 Tf 30 350 Td (EXTERIEUR) Tj ET q 1 0 0 1 30 180 cm /Fm Do Q q 1 0 0 1 230 180 cm /Fm Do Q"));
        doc.update_dict(p.obj, |d| {
            d.set(b"Resources".to_vec(), Object::Ref(resources));
            d.set(b"Contents".to_vec(), Object::Ref(stream));
        })
        .unwrap();
    }
    let e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    let object = pdfcraft_edit::page_images(&e.doc, 0).unwrap()[0]
        .object
        .unwrap();
    (e, object)
}

const ART: &[u8] = b"q 0.3 0.8 0.4 rg 5 5 30 10 re f 0 G 2 w 10 20 m 90 70 l S -20 40 m 200 40 l S BT /F1 12 Tf 20 60 Td (INTERIEUR) Tj ET Q";

fn pixels(e: &Editor, page: usize) -> Vec<u8> {
    let mut renderer = pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
    let r = renderer.render(pdfcraft_render::RenderRequest {
        page,
        scale: 1.5,
        ..Default::default()
    });
    assert!(r.error.is_none(), "{:?}", r.error);
    r.rgba.to_vec()
}

#[test]
fn dissociation_keeps_ink_resources_layers_and_shared_occurrences_then_allows_line_edits() {
    let (mut e, form) = fixture(ART, false);
    let original = e.bytes.clone();
    let before = [pixels(&e, 0), pixels(&e, 1)];
    let artwork = e.doc.get(form);
    assert!(e.drawings(0).unwrap().is_empty());
    assert!(e.can_dissociate_form(0, 0));
    e.dissociate_form(0, 0).unwrap();
    assert_eq!(
        pixels(&e, 0),
        before[0],
        "Dissociating must not repaint any pixel"
    );
    assert_eq!(
        pixels(&e, 1),
        before[1],
        "The other page must remain untouched"
    );
    assert_eq!(e.doc.get(form), artwork, "Never rewrite the shared Form");
    assert_eq!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().len(), 1);
    assert_eq!(pdfcraft_edit::page_images(&e.doc, 1).unwrap().len(), 2);
    let line = e
        .drawings(0)
        .unwrap()
        .into_iter()
        .find(|i| i.drawing.kind == Kind::Line && i.drawing.u[1].abs() > 10.)
        .unwrap();
    let mut drawing = line.drawing.clone();
    drawing.width = 4.;
    drawing.translate([8., -5.]);
    e.change_drawings(0, &[(line.target, Some(drawing))])
        .unwrap();
    assert_ne!(pixels(&e, 0), before[0]);
    assert_eq!(pixels(&e, 1), before[1]);
    e.undo().unwrap();
    assert_eq!(pixels(&e, 0), before[0]);
    e.undo().unwrap();
    assert_eq!(e.bytes, original);
    e.redo().unwrap();
    assert_eq!(pixels(&e, 0), before[0]);
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&out).unwrap();
    let temporary = tempfile::tempdir_in(out).unwrap();
    let path = temporary.path().join("schema dissocie-é.pdf");
    e.save(&path).unwrap();
    let mut reopened = Editor::open(&path).unwrap();
    assert!(!reopened.drawings(0).unwrap().is_empty());
    reopened.dissociate_form(0, 0).unwrap();
    assert!(
        pdfcraft_edit::page_images(&reopened.doc, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn transparency_unbalanced_graphics_and_invalid_targets_are_refused_atomically() {
    for (body, transparent) in [(ART, true), (b"Q 0 0 10 10 re f".as_slice(), false)] {
        let (mut e, _) = fixture(body, transparent);
        let before = e.bytes.clone();
        assert!(e.dissociate_form(0, 0).is_err());
        assert_eq!(e.bytes, before);
        assert!(!e.can_undo());
        assert!(e.dissociate_form(0, 99).is_err());
        assert_eq!(e.bytes, before);
    }
}

#[test]
fn nested_forms_keep_their_original_inherited_fonts_when_the_outer_group_is_dissociated() {
    let (e, form) = fixture(ART, false);
    let mut doc = e.doc.clone();
    let original = doc.get(form);
    let Object::Stream(outer) = original.as_ref() else {
        panic!("Form required")
    };
    let mut child = outer.dict.clone();
    child.remove(b"Resources");
    child.remove(b"Matrix");
    let nested = doc.add(Stream::flate(
        child,
        b"BT /F1 12 Tf 20 60 Td (HERITE) Tj ET",
    ));
    let mut resources = outer
        .dict
        .get(b"Resources")
        .and_then(|r| doc.dict(r))
        .unwrap();
    let mut xobjects = Dict::new();
    xobjects.set(b"Child".to_vec(), Object::Ref(nested));
    resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
    let mut dictionary = outer.dict.clone();
    dictionary.set(b"Resources".to_vec(), Object::Dict(resources));
    doc.set(
        form,
        Stream::flate(dictionary, b"/Child Do 0 G 2 w 10 20 m 90 70 l S"),
    );
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    let before = pixels(&e, 0);
    e.dissociate_form(0, 0).unwrap();
    assert_eq!(
        pixels(&e, 0),
        before,
        "A nested Form must keep its enclosing Helvetica, not the page Courier"
    );
    e.dissociate_form(0, 0).unwrap();
    assert_eq!(pixels(&e, 0), before);
    let block = e
        .blocks(0)
        .unwrap()
        .iter()
        .position(|b| b.editor_text == "HERITE")
        .unwrap();
    e.rewrite(0, block, Some("MODIFIE"), &Default::default())
        .unwrap();
    assert_ne!(pixels(&e, 0), before);
    assert!(e.text(0).unwrap().plain_text().contains("MODIFIE"));
    assert!(!e.text(1).unwrap().plain_text().contains("MODIFIE"));
}

#[test]
fn sheared_placements_stay_grouped_until_their_clips_are_editable() {
    let (mut e, _) = fixture_matrix(ART, false, [1., 0.2, 0.1, 1., 2., 3.]);
    let before = e.bytes.clone();
    assert!(!e.can_dissociate_form(0, 0));
    assert!(e.dissociate_form(0, 0).is_err());
    assert_eq!(e.bytes, before);
}

#[test]
fn nonuniform_and_quarter_turn_placements_keep_their_exact_rendering() {
    for matrix in [[1.2, 0., 0., 0.8, 2., 3.], [0., 1., -1., 0., 90., 0.]] {
        let (mut e, _) = fixture_matrix(ART, false, matrix);
        let before = pixels(&e, 0);
        assert!(e.can_dissociate_form(0, 0));
        e.dissociate_form(0, 0).unwrap();
        assert_eq!(pixels(&e, 0), before);
        assert!(
            e.selection_items(0)
                .unwrap()
                .iter()
                .any(|i| matches!(i.target, pdfinator::selection::Target::Paint(_)))
        );
    }
}

#[test]
fn split_invocations_and_existing_added_text_keep_their_stream_metadata() {
    let (e, _) = fixture(ART, false);
    let mut doc = e.doc.clone();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let a = doc.add(Stream::flate(Dict::new(), b"q 1 0 0 1 30 180 cm /Fm"));
    let b = doc.add(Stream::flate(
        Dict::new(),
        b"Do Q q 1 0 0 1 230 180 cm /Fm Do Q",
    ));
    doc.update_dict(page, |d| {
        d.set(
            b"Contents".to_vec(),
            Object::Array(vec![Object::Ref(a), Object::Ref(b)]),
        );
    })
    .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    e.add_text(
        0,
        pdfcraft_edit::AddedText {
            rect: [40., 50., 220., 80.],
            text: "Bloc ajoute".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let before = pixels(&e, 0);
    let added = e.added(0);
    e.dissociate_form(0, 0).unwrap();
    assert_eq!(pixels(&e, 0), before);
    assert_eq!(e.added(0), added);
    e.update_added(
        0,
        0,
        pdfcraft_edit::Content::Text(pdfcraft_edit::AddedText {
            rect: [40., 50., 220., 80.],
            text: "Toujours editable".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(
        e.text(0)
            .unwrap()
            .plain_text()
            .contains("Toujours editable")
    );
}

#[test]
fn damaged_and_oversized_form_streams_leave_the_original_undo_state_intact() {
    for large in [false, true] {
        let (e, form) = fixture(ART, false);
        let mut doc = e.doc.clone();
        let original = doc.get(form);
        let Object::Stream(s) = original.as_ref() else {
            panic!("Form required")
        };
        let replacement = if large {
            Stream::flate(s.dict.clone(), &vec![b' '; 8 * 1024 * 1024 + 1])
        } else {
            let mut d = s.dict.clone();
            d.set(b"Filter".to_vec(), Object::name("FlateDecode"));
            Stream::from_raw(d, vec![1, 2, 3, 4])
        };
        doc.set(form, replacement);
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
                .unwrap();
        let before = e.bytes.clone();
        assert!(e.dissociate_form(0, 0).is_err());
        assert_eq!(e.bytes, before);
        assert!(!e.can_undo());
    }
}

#[test]
fn a_copied_vector_diagram_can_be_dissociated_in_the_destination_in_one_undoable_action() {
    let (source, _) = fixture(ART, false);
    let copied = source
        .copy_image_block(
            0,
            pdfcraft_edit::ObjectTarget {
                kind: pdfcraft_edit::ObjectKind::Image,
                index: 0,
            },
        )
        .unwrap();
    let mut e = Editor::from_bytes(
        pdfcraft_cos::write_full(
            &pdfcraft_create::blank(420., 400., 1).unwrap(),
            &Default::default(),
        )
        .unwrap(),
    )
    .unwrap();
    let index = e.paste_image_block(0, &copied, [100., 60.]).unwrap();
    let before = e.bytes.clone();
    let ink = pixels(&e, 0);
    assert!(e.can_dissociate_form(0, index));
    e.dissociate_form(0, index).unwrap();
    assert_eq!(pixels(&e, 0), ink);
    assert!(pdfcraft_edit::page_images(&e.doc, 0).unwrap().is_empty());
    assert!(!e.drawings(0).unwrap().is_empty());
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
}

#[test]
fn cyclic_group_wrappers_are_bounded_and_cannot_publish_a_partial_change() {
    let (e, form) = fixture(ART, false);
    let mut doc = e.doc.clone();
    let original = doc.get(form);
    let Object::Stream(s) = original.as_ref() else {
        panic!("Form required")
    };
    let mut resources = Dict::new();
    let mut xo = Dict::new();
    xo.set(b"Loop".to_vec(), Object::Ref(form));
    resources.set(b"XObject".to_vec(), Object::Dict(xo));
    let mut dictionary = s.dict.clone();
    dictionary.set(b"Resources".to_vec(), Object::Dict(resources));
    dictionary.remove(b"Matrix");
    doc.set(form, Stream::flate(dictionary, b"/Loop Do"));
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    let before = e.bytes.clone();
    assert!(e.dissociate_form(0, 0).is_err());
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
}
