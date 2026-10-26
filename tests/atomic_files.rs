use pdfinator::files::{atomic_write, read};

#[test]
fn atomic_write_replaces_existing_content_without_leftover_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document-école.pdf");
    std::fs::write(&path, b"original").unwrap();
    atomic_write(&path, b"new PDF bytes").unwrap();
    assert_eq!(read(&path).unwrap(), b"new PDF bytes");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn atomic_write_handles_empty_payload_and_unicode_paths() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("évaluation-3e-été.pdf");
    atomic_write(&path, b"").unwrap();
    assert!(read(&path).unwrap().is_empty());
    atomic_write(&path, "PDF - français".as_bytes()).unwrap();
    assert_eq!(read(&path).unwrap(), "PDF - français".as_bytes());
}

#[test]
fn atomic_write_failure_leaves_existing_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.pdf");
    std::fs::write(&path, b"keep me").unwrap();
    let missing = dir.path().join("missing").join("original.pdf");
    assert!(atomic_write(&missing, b"replacement").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
}

#[test]
fn refuses_oversized_pdf_before_allocating_its_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("oversized.pdf");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 * 1024 + 1).unwrap();
    assert!(read(&path).unwrap_err().contains("256 Mo"));
}
