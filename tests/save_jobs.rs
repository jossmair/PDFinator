use pdfinator::document::Editor;
#[test]
fn save_as_restores_file_identity_and_case_alias_detects_external_changes() {
    let dir = tempfile::tempdir_in("test-output").unwrap();
    let path = dir.path().join("original.pdf");
    let new_path = dir.path().join("renamed.pdf");
    let d = pdfcraft_create::blank(595., 842., 1).unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
    e.save(&path).unwrap();
    e.save(&new_path).unwrap();
    assert_eq!(e.canonical_path, Some(new_path.canonicalize().unwrap()));
    #[cfg(windows)]
    {
        let alias = dir.path().join("RENAMED.PDF");
        assert_eq!(
            new_path.canonicalize().unwrap(),
            alias.canonicalize().unwrap()
        );
        std::fs::write(&new_path, b"external newer document").unwrap();
        assert!(
            e.prepare_save(&alias)
                .write()
                .unwrap_err()
                .contains("modifié ailleurs")
        );
        assert_eq!(
            std::fs::read(&new_path).unwrap(),
            b"external newer document"
        );
    }
}
#[test]
fn a_save_snapshot_never_marks_later_edits_saved_or_overwrites_external_changes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
    let dir = tempfile::tempdir_in(root).unwrap();
    let path = dir.path().join("travail.pdf");
    let d = pdfcraft_create::blank(595., 842., 1).unwrap();
    let mut e =
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
    e.insert_blank_after(0).unwrap();
    let task = e.prepare_save(&path);
    e.insert_blank_after(1).unwrap();
    task.write().unwrap();
    e.accept_save(&task);
    assert!(e.dirty());
    assert_eq!(e.info.pages.len(), 3);
    assert_eq!(Editor::open(&path).unwrap().info.pages.len(), 2);
    e.save(&path).unwrap();
    assert!(!e.dirty());
    assert_eq!(Editor::open(&path).unwrap().info.pages.len(), 3);
    let task = e.prepare_save(&path);
    std::fs::write(&path, b"changed by another program").unwrap();
    assert!(task.write().unwrap_err().contains("modifié ailleurs"));
    assert_eq!(std::fs::read(&path).unwrap(), b"changed by another program");
}
