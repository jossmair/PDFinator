use pdfcraft_cos::{Dict, Object, Stream};
use pdfinator::document::Editor;

#[test]
fn offset_fake_bold_layers_move_together() {
    let mut doc = pdfcraft_create::blank(595., 842., 1).unwrap();
    let mut font = Dict::new();
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(font));
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let stream = doc.add(Stream::flate(
        Dict::new(),
        b"BT /F1 20 Tf 50 700 Td (LABEL) Tj ET BT /F1 20 Tf 50.7 700 Td (LABEL) Tj ET",
    ));
    let page = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(page, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(stream));
    })
    .unwrap();
    let mut editor =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    let before = pdfcraft_edit::text_lines(&editor.doc, 0).unwrap();
    assert_eq!(before.len(), 2);
    editor.move_block(0, 0, [12., 8.]).unwrap();
    let after = pdfcraft_edit::text_lines(&editor.doc, 0).unwrap();
    assert_eq!(after.len(), 2);
    for (a, b) in before.iter().zip(&after) {
        assert!((b.baseline()[0] - a.baseline()[0] - 12.).abs() < 0.01);
        assert!((b.baseline()[1] - a.baseline()[1] - 8.).abs() < 0.01);
    }
}

#[test]
fn filled_and_outlined_same_glyph_is_one_editable_character() {
    let mut doc = pdfcraft_create::blank(595., 842., 1).unwrap();
    let mut font = Dict::new();
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(font));
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let content = doc.add(Stream::flate(Dict::new(), b"BT /F1 20 Tf 1 0 0 1 50 700 Tm (T) Tj ET BT /F1 20 Tf 1 Tr 1 0 0 1 50 700 Tm (T) Tj 0 Tr (RIANGLE) Tj ET"));
    let page = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(page, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(content));
    })
    .unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap();
    let blocks = e.blocks(0).unwrap();
    assert_eq!(blocks[0].editor_text, "TRIANGLE");
    e.rewrite(0, 0, Some("RECTANGLE"), &Default::default())
        .unwrap();
    assert_eq!(e.blocks(0).unwrap()[0].editor_text, "RECTANGLE");
    assert!(!e.text(0).unwrap().plain_text().contains("TRECTANGLE"));
}
