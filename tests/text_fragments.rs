use pdfcraft_cos::{Dict, Object, Stream};
use pdfinator::document::Editor;

fn source(content: &[u8]) -> Editor {
    let mut doc = pdfcraft_create::blank(420., 595., 1).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Ref(doc.add(font)));
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let content = doc.add(Stream::flate(Dict::new(), content));
    doc.update_dict(page, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(content));
    })
    .unwrap();
    Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap()
}

#[test]
fn fragmented_native_paragraph_rejoins_interleaved_runs_and_edits_once() {
    let paragraph = b"BT /F1 12 Tf 1 0 0 1 50 500 Tm (Un texte ) Tj ET BT /F1 12 Tf 1 0 0 1 50 485 Tm (Suite du paragraphe.) Tj ET BT /F1 12 Tf 1 0 0 1 101 500 Tm (proche) Tj ET";
    for tagged in [false, true] {
        let mut content = Vec::new();
        if tagged {
            content.extend_from_slice(b"/P BMC ");
        }
        content.extend_from_slice(paragraph);
        if tagged {
            content.extend_from_slice(b" EMC ");
        }
        content.extend_from_slice(b" BT /F1 12 Tf 1 0 0 1 300 400 Tm (Voisin intact) Tj ET");
        let mut e = source(&content);
        let before = e.bytes.clone();
        let blocks = e.blocks(0).unwrap();
        assert_eq!(
            blocks.len(),
            2,
            "Fragments of the same paragraph must share a frame: {blocks:?}"
        );
        let index = blocks
            .iter()
            .position(|b| b.editor_text.contains("Un texte"))
            .unwrap();
        assert!(
            blocks[index]
                .editor_text
                .contains("proche\nSuite du paragraphe.")
        );
        e.rewrite(0, index, Some("Paragraphe corrige."), &Default::default())
            .unwrap();
        let text = e.text(0).unwrap().plain_text();
        assert!(text.contains("Paragraphe corrige."));
        assert!(text.contains("Voisin intact"));
        assert!(!text.contains("Suite du paragraphe"));
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
        e.redo().unwrap();
        let reopened = Editor::from_bytes(e.bytes.as_ref().clone()).unwrap();
        assert!(
            reopened
                .text(0)
                .unwrap()
                .plain_text()
                .contains("Voisin intact")
        );
    }
}

#[test]
fn nearby_owned_frames_and_distant_columns_remain_independently_editable() {
    let mut e = source(b"BT /F1 12 Tf 50 500 Td (Colonne gauche) Tj ET BT /F1 12 Tf 250 500 Td (Colonne droite) Tj ET");
    assert_eq!(e.blocks(0).unwrap().len(), 2);
    for (id, text, rect) in [
        ("frame-a", "Premier cadre", [50., 330., 160., 350.]),
        ("frame-b", "Deuxieme cadre", [50., 315., 160., 335.]),
    ] {
        e.format_text(
            0,
            None,
            None,
            pdfinator::rich::TextData {
                id: id.into(),
                text: text.into(),
                rect,
                base_size: 12.,
                base_color: [0.; 3],
                base_bold: false,
                base_italic: false,
                font_name: "Helvetica".into(),
                align: None,
                line_spacing: None,
                spans: vec![],
            },
            &[],
        )
        .unwrap();
    }
    let blocks = e.blocks(0).unwrap();
    let first = blocks
        .iter()
        .position(|b| b.editor_text == "Premier cadre")
        .unwrap();
    assert!(blocks.iter().any(|b| b.editor_text == "Deuxieme cadre"));
    e.rewrite(0, first, Some("Premier corrige"), &Default::default())
        .unwrap();
    let text = e.text(0).unwrap().plain_text();
    assert!(
        text.contains("Deuxieme cadre")
            && text.contains("Colonne droite")
            && text.contains("Colonne gauche")
    );
}
