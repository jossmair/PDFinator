use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
#[derive(Default)]
pub(super) struct Control {
    cancelled: AtomicBool,
    finalizing: AtomicBool,
    finished: AtomicBool,
    completed: AtomicUsize,
    total: AtomicUsize,
    commit: Mutex<()>,
}
impl Control {
    fn cancel(&self) -> bool {
        if let Ok(_guard) = self.commit.try_lock() {
            if self.finished.load(Ordering::Acquire) {
                return false;
            }
            self.cancelled.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
}
pub(super) struct Job {
    rx: mpsc::Receiver<Result<PathBuf>>,
    control: Arc<Control>,
}

impl App {
    pub(super) fn export_word(&mut self, ctx: &egui::Context, format: pdfinator::docx::Format) {
        if self.word_export.is_some() {
            return;
        }
        self.commit_text_keep_focus(ctx);
        if self
            .inline
            .as_ref()
            .is_some_and(|i| !i.committed && i.changed())
        {
            return;
        }
        let Some(editor) = &self.editor else {
            return;
        };
        let name = editor
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .unwrap_or(std::ffi::OsStr::new("Document"))
            .to_string_lossy();
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!("Exporter en {}", format.label()))
            .set_file_name(format!("{name}.{}", format.extension()))
            .add_filter(format.label(), &[format.extension()])
            .save_file()
        else {
            return;
        };
        self.start_word_export(ctx, path.with_extension(format.extension()));
    }
    pub(super) fn start_word_export(&mut self, ctx: &egui::Context, path: PathBuf) {
        let Some(editor) = &self.editor else {
            return;
        };
        if self.word_export.is_some() {
            return;
        }
        let bytes = editor.bytes.clone();
        let format = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("odt"))
        {
            pdfinator::docx::Format::Odt
        } else {
            pdfinator::docx::Format::Docx
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let control = Arc::new(Control::default());
        self.word_export = Some(Job {
            rx,
            control: control.clone(),
        });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = pdfinator::docx::convert_with_progress(
                bytes.as_ref().clone(),
                format,
                &control.cancelled,
                |done, total| {
                    control.total.store(total, Ordering::Release);
                    control.completed.store(done, Ordering::Release);
                    ctx.request_repaint();
                },
            )
            .and_then(|bytes| {
                control.finalizing.store(true, Ordering::Release);
                let _commit = control
                    .commit
                    .lock()
                    .map_err(|_| "L’export s’est interrompu.".to_string())?;
                if control.cancelled.load(Ordering::Acquire) {
                    return Err("Export annulé.".into());
                }
                pdfinator::files::atomic_write(&path, &bytes)?;
                control.finished.store(true, Ordering::Release);
                Ok(path)
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_word_export(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.word_export {
            match job.rx.try_recv() {
                Ok(result) => {
                    self.word_export = None;
                    if result.is_err() {
                        self.work.exit_after_jobs = false;
                    }
                    self.message = Some(match result {
                        Ok(path) => format!("Document créé :\n{}", path.display()),
                        Err(error) => format!("Export impossible : {error}"),
                    });
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.word_export = None;
                    self.work.exit_after_jobs = false;
                    self.message = Some("L’export s’est interrompu. Le PDF est conservé.".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }
    }
    pub(super) fn export_status(&mut self, root: &mut egui::Ui) {
        let Some(job) = &self.word_export else { return };
        let total = job.control.total.load(Ordering::Acquire);
        let done = job.control.completed.load(Ordering::Acquire);
        egui::Panel::bottom("export-progress").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                let cancelled = job.control.cancelled.load(Ordering::Acquire);
                let label = if cancelled {
                    "Annulation en cours…".into()
                } else if total == 0 {
                    "Préparation de l’export…".into()
                } else if done == total {
                    "Finalisation du document…".into()
                } else {
                    format!("Conversion : page {} sur {total}", done + 1)
                };
                ui.label(label);
                if total > 0 {
                    ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(140.));
                }
                if ui
                    .add_enabled(
                        !cancelled && !job.control.finalizing.load(Ordering::Acquire),
                        egui::Button::new("Annuler l’export"),
                    )
                    .clicked()
                {
                    job.control.cancel();
                }
            });
        });
        root.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_window_page_counter_stays_at_zero() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::from_context(&ctx, None);
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        assert!(
            nodes
                .iter()
                .any(|(_, node)| node.numeric_value() == Some(0.0)),
            "{:?}", nodes.iter().map(|(_, n)| (n.role(), n.label(), n.value(), n.numeric_value())).collect::<Vec<_>>()
        );
        assert!(
            !nodes
                .iter()
                .any(|(_, node)| node.numeric_value() == Some(1.0))
        );
    }
    #[test]
    fn closing_waits_for_export_and_failure_keeps_the_window_open() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let (tx, rx) = mpsc::channel();
        app.word_export = Some(Job {
            rx,
            control: Arc::new(Control::default()),
        });
        app.request_exit(&ctx);
        assert!(!app.close_confirmed);
        assert!(app.work.exit_after_jobs);
        tx.send(Err("destination inaccessible".into())).unwrap();
        app.poll_word_export(&ctx);
        assert!(!app.work.exit_after_jobs);
        assert!(!app.close_confirmed);
        assert!(
            app.message
                .as_ref()
                .unwrap()
                .contains("destination inaccessible")
        );
    }
    #[test]
    fn closing_after_a_successful_export_completes_only_when_the_result_is_received() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let (tx, rx) = mpsc::channel();
        app.word_export = Some(Job {
            rx,
            control: Arc::new(Control::default()),
        });
        app.request_exit(&ctx);
        assert!(!app.close_confirmed);
        assert!(app.work.exit_after_jobs);
        tx.send(Ok(PathBuf::from("completed.docx"))).unwrap();
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        assert!(app.close_confirmed);
        assert!(!app.work.exit_after_jobs);
    }
    #[test]
    fn cancel_discard_during_an_existing_save_never_requests_a_deferred_close() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let (_tx, rx) = mpsc::channel();
        app.work.saving = Some(rx);
        assert_eq!(
            app.resolve_discard_choice(rfd::MessageDialogResult::Cancel),
            CloseDecision::Cancel
        );
        assert_eq!(app.work.exit_after_save, None);
        assert_eq!(app.work.close_after_save, None);
        assert!(!app.work.exit_after_jobs);
        assert!(!app.close_confirmed);
    }
    #[test]
    fn cancelling_an_export_preserves_an_existing_destination() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let d = pdfcraft_create::blank(595., 842., 3).unwrap();
        app.editor = Some(
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
        );
        let dir = tempfile::tempdir_in(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output"),
        )
        .unwrap();
        let path = dir.path().join("existing.docx");
        std::fs::write(&path, b"existing file").unwrap();
        app.start_word_export(&ctx, path.clone());
        assert!(app.word_export.as_ref().unwrap().control.cancel());
        for _ in 0..1000 {
            app.poll_word_export(&ctx);
            if app.word_export.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.word_export.is_none());
        assert_eq!(std::fs::read(path).unwrap(), b"existing file");
        assert!(app.message.as_ref().is_some_and(|m| m.contains("annulé")));
    }
    #[test]
    fn cancellation_is_not_acknowledged_while_the_file_commit_owns_the_gate() {
        let control = Control::default();
        let guard = control.commit.lock().unwrap();
        assert!(!control.cancel());
        drop(guard);
        assert!(control.cancel());
    }
    #[test]
    fn both_progress_bars_leave_the_bottom_toolbar_reachable_in_a_small_window() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::from_context(&ctx, None);
        app.workspace = Workspace::Edit;
        let d = pdfcraft_create::blank(595., 842., 1).unwrap();
        app.editor = Some(
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
        );
        let (_export_tx, rx) = mpsc::channel();
        app.word_export = Some(Job {
            rx,
            control: Arc::new(Control::default()),
        });
        let (_save_tx, rx) = mpsc::channel();
        app.work.saving = Some(rx);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(760., 540.));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        output.textures_delta.clear();
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        for label in ["Page entière", "Adapter à la largeur"] {
            let node = nodes
                .iter()
                .find(|(_, n)| n.label() == Some(label))
                .unwrap_or_else(|| panic!("missing {label}"));
            let b = node.1.bounds().unwrap();
            assert!(b.y1 <= 540. && b.y0 >= 0., "{label}: {b:?}");
        }
    }
}
