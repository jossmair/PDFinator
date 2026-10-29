use pdfcraft_cos::{Dict, Document, Object};
use pdfcraft_edit::{AddedText, Content};
use pdfinator::document::Editor;

fn source() -> Editor {
    let mut doc = pdfcraft_create::blank(420., 595., 5).unwrap();
    for page in 0..5 {
        pdfcraft_edit::add_content(
            &mut doc,
            page,
            &Content::Text(AddedText {
                rect: [30., 40., 300., 80.],
                text: format!("Identite {page}"),
                ..Default::default()
            }),
        )
        .unwrap();
    }
    let pages = pdfcraft_model::pages(&doc);
    doc.update_dict(pages[3].obj, |d| {
        d.set(
            b"MediaBox".to_vec(),
            Object::Array(vec![0.into(), 0.into(), 842.into(), 595.into()]),
        );
        d.set(b"Rotate".to_vec(), 90);
    })
    .unwrap();
    let mut link = Dict::new();
    link.set(b"Type".to_vec(), Object::name("Annot"));
    link.set(b"Subtype".to_vec(), Object::name("Link"));
    link.set(
        b"Rect".to_vec(),
        Object::Array(vec![10.into(), 10.into(), 50.into(), 30.into()]),
    );
    link.set(b"P".to_vec(), Object::Ref(pages[0].obj));
    link.set(
        b"Dest".to_vec(),
        Object::Array(vec![Object::Ref(pages[3].obj), Object::name("Fit")]),
    );
    let link = doc.add(Object::Dict(link));
    doc.update_dict(pages[0].obj, |d| {
        d.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(link)]))
    })
    .unwrap();
    pdfcraft_organize::add_bookmark(&mut doc, &[], 0, "Page cible", 3).unwrap();
    let mut editor =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    editor
        .add_comment(3, [100., 100.], "Commentaire de la page 3")
        .unwrap();
    Editor::from_bytes(editor.bytes.as_ref().clone()).unwrap()
}

fn order(e: &Editor) -> Vec<usize> {
    (0..e.info.pages.len())
        .map(|page| {
            let text = e.text(page).unwrap().plain_text();
            (0..5)
                .find(|n| text.contains(&format!("Identite {n}")))
                .unwrap()
        })
        .collect()
}
fn reopened(e: &Editor) -> Editor {
    Editor::from_bytes(e.bytes.as_ref().clone()).unwrap()
}
fn link_destination(doc: &Document, page: usize) -> pdfcraft_cos::ObjRef {
    let pages = pdfcraft_model::pages(doc);
    let annots = doc.resolve(pages[page].dict.get(b"Annots").unwrap());
    let link = annots
        .as_array()
        .unwrap()
        .iter()
        .map(|a| doc.resolve(a))
        .find(|a| {
            a.as_dict()
                .is_some_and(|d| d.name(b"Subtype") == Some(b"Link"))
        })
        .unwrap();
    link.as_dict()
        .unwrap()
        .get(b"Dest")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_ref()
        .unwrap()
}

#[test]
fn disjoint_page_move_preserves_links_comments_geometry_and_one_step_history() {
    let mut e = source();
    let before = e.bytes.clone();
    let target = pdfcraft_model::pages(&e.doc)[3].obj;
    e.move_page_group(&[3, 0, 3], 1).unwrap();
    assert_eq!(order(&e), [1, 0, 3, 2, 4]);
    assert_eq!(link_destination(&e.doc, 1), target);
    assert_eq!(pdfcraft_organize::bookmarks(&e.doc)[0].page, Some(2));
    assert!(
        e.info
            .annotations
            .iter()
            .any(|a| a.page == 2 && a.contents.as_deref() == Some("Commentaire de la page 3"))
    );
    let after = e.bytes.clone();
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.bytes, after);
    let e = reopened(&e);
    assert_eq!(order(&e), [1, 0, 3, 2, 4]);
    assert_eq!(
        link_destination(&e.doc, 1),
        pdfcraft_model::pages(&e.doc)[2].obj
    );
    assert!((e.info.pages[2].height - 842.).abs() < 0.1);
}

#[test]
fn duplicated_group_has_independent_annotations_and_internal_destinations() {
    let mut e = source();
    let before = e.bytes.clone();
    assert_eq!(e.duplicate_page_group(&[3, 0]).unwrap(), 4..6);
    assert_eq!(order(&e), [0, 1, 2, 3, 0, 3, 4]);
    let pages = pdfcraft_model::pages(&e.doc);
    assert_eq!(link_destination(&e.doc, 0), pages[3].obj);
    assert_eq!(link_destination(&e.doc, 4), pages[5].obj);
    let original = pages[3].dict.get(b"Annots").unwrap();
    let copy = pages[5].dict.get(b"Annots").unwrap();
    assert_ne!(original, copy);
    assert!(
        e.info
            .annotations
            .iter()
            .any(|a| a.page == 5 && a.contents.as_deref() == Some("Commentaire de la page 3"))
    );
    let e2 = reopened(&e);
    assert_eq!(order(&e2), [0, 1, 2, 3, 0, 3, 4]);
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
}

#[test]
fn group_deletion_is_atomic_protects_last_page_and_removes_dead_links() {
    let mut e = source();
    let before = e.bytes.clone();
    for pages in [vec![], vec![5], vec![0, 1, 2, 3, 4]] {
        assert!(e.delete_page_group(&pages).is_err());
        assert_eq!(e.bytes, before);
        assert!(!e.can_undo());
    }
    e.delete_page_group(&[1, 3]).unwrap();
    assert_eq!(order(&e), [0, 2, 4]);
    let pages = pdfcraft_model::pages(&e.doc);
    let annots = e.doc.resolve(pages[0].dict.get(b"Annots").unwrap());
    for annot in annots.as_array().unwrap() {
        assert!(
            e.doc
                .resolve(annot)
                .as_dict()
                .unwrap()
                .get(b"Dest")
                .is_none()
        );
    }
    let after = e.bytes.clone();
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.bytes, after);
    assert_eq!(order(&reopened(&e)), [0, 2, 4]);
}

#[test]
fn group_rotation_keeps_editable_text_and_undoes_all_pages_together() {
    let mut e = source();
    let before = e.bytes.clone();
    e.rotate_page_group(&[0, 3], 90).unwrap();
    assert!((e.info.pages[0].width - 595.).abs() < 0.1);
    assert!((e.info.pages[3].width - 842.).abs() < 0.1);
    assert!((e.info.pages[1].width - 420.).abs() < 0.1);
    assert_eq!(order(&reopened(&e)), [0, 1, 2, 3, 4]);
    assert!(!e.blocks(0).unwrap().is_empty());
    e.undo().unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
}

#[test]
fn invalid_and_identity_group_operations_do_not_dirty_the_source() {
    let mut e = source();
    let before = e.bytes.clone();
    assert!(e.move_page_group(&[0, 2], 4).is_err());
    assert!(e.move_page_group(&[], 0).is_err());
    assert!(e.duplicate_page_group(&[5]).is_err());
    assert!(e.rotate_page_group(&[0, 5], 90).is_err());
    assert!(e.rotate_page_group(&[0], 45).is_err());
    e.move_page_group(&[0, 1], 0).unwrap();
    e.move_page_group(&[4, 3, 2, 1, 0], 0).unwrap();
    e.rotate_page_group(&[0, 3], 360).unwrap();
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    assert!(!e.dirty());
}

#[test]
fn extraction_keeps_editable_contents_links_and_comments_without_touching_source() {
    let e = source();
    let before = e.bytes.clone();
    let bytes = e.extract_page_group(&[3, 0, 3]).unwrap();
    let mut extracted = Editor::from_generated_bytes(bytes, "Pages extraites.pdf".into()).unwrap();
    assert_eq!(order(&extracted), [0, 3]);
    assert_eq!(
        link_destination(&extracted.doc, 0),
        pdfcraft_model::pages(&extracted.doc)[1].obj
    );
    assert!(
        extracted
            .info
            .annotations
            .iter()
            .any(|a| a.page == 1 && a.contents.as_deref() == Some("Commentaire de la page 3"))
    );
    assert!(extracted.dirty());
    assert!(!extracted.can_undo());
    assert!(extracted.path.is_none());
    extracted
        .add_text(
            0,
            AddedText {
                rect: [30., 200., 300., 240.],
                text: "Nouvelle copie".into(),
                ..Default::default()
            },
        )
        .unwrap();
    extracted.undo().unwrap();
    assert!(extracted.dirty());
    assert_eq!(e.bytes, before);
    assert!(!e.dirty());
    assert!(!e.can_undo());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    extracted.save(&dir.path().join("Pages été.pdf")).unwrap();
    assert!(!extracted.dirty());
    assert_eq!(
        order(&Editor::open(&dir.path().join("Pages été.pdf")).unwrap()),
        [0, 3]
    );
}

#[test]
fn group_moves_keep_form_widget_page_and_field_relationships_after_reopen() {
    let mut e = source();
    let pages = pdfcraft_model::pages(&e.doc);
    let mut widget = Dict::new();
    for (key, value) in [("Type", "Annot"), ("Subtype", "Widget"), ("FT", "Tx")] {
        widget.set(key.as_bytes().to_vec(), Object::name(value));
    }
    widget.set(
        b"T".to_vec(),
        Object::String(pdfcraft_cos::PdfString::text("Nom")),
    );
    widget.set(
        b"V".to_vec(),
        Object::String(pdfcraft_cos::PdfString::text("Eleve")),
    );
    widget.set(b"P".to_vec(), Object::Ref(pages[2].obj));
    widget.set(
        b"Rect".to_vec(),
        Object::Array(vec![20.into(), 200.into(), 100.into(), 220.into()]),
    );
    let widget = e.doc.add(widget);
    e.doc
        .update_dict(pages[2].obj, |d| {
            d.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(widget)]))
        })
        .unwrap();
    let mut form = Dict::new();
    form.set(b"Fields".to_vec(), Object::Array(vec![Object::Ref(widget)]));
    e.doc
        .update_dict(e.doc.root().unwrap(), |d| {
            d.set(b"AcroForm".to_vec(), Object::Dict(form))
        })
        .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap();
    e.move_page_group(&[0, 2], 3).unwrap();
    let e = reopened(&e);
    let pages = pdfcraft_model::pages(&e.doc);
    assert_eq!(order(&e), [1, 3, 4, 0, 2]);
    let form = e
        .doc
        .get(e.doc.root().unwrap())
        .as_dict()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .clone();
    let form = e.doc.resolve(&form);
    let widget = form
        .as_dict()
        .unwrap()
        .get(b"Fields")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_ref()
        .unwrap();
    let field = e.doc.get(widget);
    assert_eq!(field.as_dict().unwrap().reference(b"P"), Some(pages[4].obj));
    let annots = e.doc.resolve(pages[4].dict.get(b"Annots").unwrap());
    assert_eq!(annots.as_array().unwrap()[0].as_ref(), Some(widget));
    assert_eq!(
        field.as_dict().unwrap().get(b"V").unwrap(),
        &Object::String(pdfcraft_cos::PdfString::text("Eleve"))
    );
}

#[test]
fn duplicate_and_extracted_pages_render_identically_to_their_sources() {
    let mut e = source();
    let pixels = |e: &Editor, page| {
        let mut renderer = pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
        let result = renderer.render(pdfcraft_render::RenderRequest {
            page,
            scale: 0.8,
            ..Default::default()
        });
        assert!(result.error.is_none(), "{:?}", result.error);
        result.rgba.to_vec()
    };
    let expected = [pixels(&e, 0), pixels(&e, 3)];
    assert!(expected.iter().all(|p| p.iter().any(|&v| v < 180)));
    let copy = Editor::from_bytes(e.extract_page_group(&[0, 3]).unwrap()).unwrap();
    assert_eq!(pixels(&copy, 0), expected[0]);
    assert_eq!(pixels(&copy, 1), expected[1]);
    e.duplicate_page_group(&[0, 3]).unwrap();
    assert_eq!(pixels(&e, 4), expected[0]);
    assert_eq!(pixels(&e, 5), expected[1]);
}

#[test]
fn signed_document_group_edits_require_acceptance_and_fail_atomically() {
    let mut e = source();
    let signature = e.doc.add({
        let mut d = Dict::new();
        d.set(
            b"ByteRange".to_vec(),
            Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]),
        );
        d
    });
    e.doc
        .trailer_mut()
        .set(b"TestSignature".to_vec(), Object::Ref(signature));
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap();
    assert!(e.signed);
    let before = e.bytes.clone();
    assert!(e.move_page_group(&[0, 2], 2).is_err());
    assert!(e.duplicate_page_group(&[0, 2]).is_err());
    assert!(e.delete_page_group(&[0, 2]).is_err());
    assert!(e.rotate_page_group(&[0, 2], 90).is_err());
    assert_eq!(e.bytes, before);
    assert!(!e.can_undo());
    assert!(!e.dirty());
    e.signature_accepted = true;
    e.move_page_group(&[0, 2], 2).unwrap();
    assert_eq!(order(&e), [1, 3, 0, 2, 4]);
}
