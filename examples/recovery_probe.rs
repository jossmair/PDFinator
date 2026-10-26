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
        let styles=serde_json::json!({"bold":true,"italic":true,"size":19.5,"line_spacing":1.7,"color":[0.2,0.3,0.7]}).to_string();
        journal.observe_state(
            editor.instance,
            editor.revision(),
            editor.bytes.clone(),
            "Cours.pdf".into(),
            "Brouillon : été œ".into(),
            true,
            Some(pdfinator::recovery::PendingState {
                key: styles.clone(),
                encode: std::sync::Arc::new(move || Ok(styles.clone())),
            }),
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
        let styles: serde_json::Value =
            serde_json::from_str(entry.metadata.state.as_ref().expect("formatted state")).unwrap();
        assert_eq!(styles["bold"], true);
        assert_eq!(styles["italic"], true);
        assert_eq!(styles["line_spacing"], 1.7);
        assert_eq!(styles["size"], 19.5);
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
            "RECOVERY_VERIFIED: 2 pages, Unicode draft and formatting, original process exited without cleanup"
        );
        journal.clean_exit();
    }
}
