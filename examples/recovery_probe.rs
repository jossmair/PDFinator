//! Cross-process recovery check. Only operates in an explicitly supplied QA directory.
use pdfinator::{document::Editor, recovery::Journal};
fn main() {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().expect("mode");
    let root = std::path::PathBuf::from(args.next().expect("QA directory"));
    let mut journal = Journal::start(&root).expect("journal");
    if mode == "write" {
        let doc = pdfcraft_create::from_text("", "Travail a recuperer", (595., 842.), 14.).unwrap();
        let mut editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
                .unwrap();
        editor.insert_blank_after(0).unwrap();
        journal.observe(
            editor.instance,
            editor.revision(),
            editor.bytes.clone(),
            "Cours.pdf".into(),
            "Brouillon : été œ".into(),
            true,
        );
        let complete = || {
            std::fs::read_dir(&root).unwrap().flatten().any(|d| {
                std::fs::read_dir(d.path()).is_ok_and(|files| {
                    files
                        .flatten()
                        .any(|f| f.path().extension().is_some_and(|e| e == "json"))
                })
            })
        };
        for _ in 0..100 {
            if complete() {
                println!("CHECKPOINT_READY");
                std::process::exit(77);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("checkpoint not written");
    } else {
        assert_eq!(journal.entries.len(), 1);
        let entry = &journal.entries[0];
        assert_eq!(entry.metadata.draft, "Brouillon : été œ");
        let editor = Editor::open(&entry.pdf).unwrap();
        assert_eq!(editor.info.pages.len(), 2);
        assert!(
            editor
                .text(0)
                .unwrap()
                .plain_text()
                .contains("Travail a recuperer")
        );
        println!(
            "RECOVERY_VERIFIED: 2 pages and Unicode draft, original process exited without cleanup"
        );
        journal.clean_exit();
    }
}
