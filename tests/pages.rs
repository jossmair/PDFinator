use pdfcraft_cos::{Object, SaveOptions};
use pdfcraft_edit::{AddedText, Content};
use pdfinator::document::Editor;

#[test]
fn blank_page_follows_the_active_page_size_and_orientation_and_is_undoable_after_reopen() {
    for rotation in [0, 90, 180, 270] {
        let mut doc = pdfcraft_create::blank(595., 842., 2).unwrap();
        for page in 0..2 {
            pdfcraft_edit::add_content(
                &mut doc,
                page,
                &Content::Text(AddedText {
                    rect: [60., 700., 400., 740.],
                    text: format!("Page initiale {}", page + 1),
                    ..Default::default()
                }),
            )
            .unwrap();
        }
        let page = pdfcraft_model::pages(&doc)[0].obj;
        doc.update_dict(page, |d| {
            d.set(b"Rotate".to_vec(), Object::Int(rotation));
            d.set(b"UserUnit".to_vec(), Object::Real(1.5));
            d.set(
                b"CropBox".to_vec(),
                Object::Array(vec![20.into(), 30.into(), 575.into(), 812.into()]),
            );
        })
        .unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap())
                .unwrap();
        let before = e.bytes.clone();
        let size = (e.info.pages[0].width, e.info.pages[0].height);
        assert!(e.insert_blank_after(9).is_err());
        assert_eq!(e.bytes, before);
        assert_eq!(e.insert_blank_after(0).unwrap(), 1);
        assert_eq!(e.info.pages.len(), 3);
        assert_eq!(
            pdfcraft_model::pages(&e.doc)[1]
                .dict
                .get(b"UserUnit")
                .and_then(Object::as_f64),
            Some(1.5)
        );
        assert_eq!((e.info.pages[1].width, e.info.pages[1].height), size);
        assert!(e.text(1).unwrap().plain_text().trim().is_empty());
        assert!(e.text(2).unwrap().plain_text().contains("Page initiale 2"));
        let after = e.bytes.clone();
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
        assert!(!e.can_undo());
        e.redo().unwrap();
        assert_eq!(e.bytes, after);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test-output/25-page-blanche-{rotation}.pdf"));
        e.save(&path).unwrap();
        let mut reopened = Editor::open(&path).unwrap();
        assert_eq!(reopened.info.pages.len(), 3);
        assert_eq!(
            (reopened.info.pages[1].width, reopened.info.pages[1].height),
            size
        );
        reopened
            .add_text(
                1,
                AddedText {
                    rect: [60., 150., 500., 190.],
                    text: "Page blanche devenue modifiable".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            reopened
                .text(1)
                .unwrap()
                .plain_text()
                .contains("devenue modifiable")
        );
    }
}
