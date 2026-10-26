use super::*;
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
type SearchResult = Result<(
    Vec<(usize, std::ops::Range<usize>)>,
    Vec<(usize, Arc<PageText>)>,
)>;
pub(super) struct SearchJob {
    pub receiver: Receiver<SearchResult>,
    pub instance: u64,
    pub bytes: Arc<Vec<u8>>,
    pub query: String,
    pub cancelled: Arc<std::sync::atomic::AtomicBool>,
}
pub(super) struct Work {
    pub panel_width: f32,
    pub fit_compensation: f32,
    pub help_open: bool,
    pub opening: Option<Receiver<Result<Editor>>>,
    pub opening_path: Option<PathBuf>,
    pub paths: VecDeque<PathBuf>,
    pub ready: Option<Editor>,
    pub saving: Option<Receiver<Result<pdfinator::document::SaveTask>>>,
    pub close_after_save: Option<u64>,
    pub exit_after_save: Option<u64>,
    pub exit_after_jobs: bool,
    pub searching: Option<SearchJob>,
    pub search_due: Option<Instant>,
    pub journal: pdfinator::recovery::Journal,
    pub show_recovery: bool,
    pub recovered_draft: Option<String>,
    pub recovering_from: Option<PathBuf>,
}
impl Default for Work {
    fn default() -> Self {
        Self {
            panel_width: 0.,
            fit_compensation: 0.,
            help_open: false,
            opening: None,
            opening_path: None,
            paths: VecDeque::new(),
            ready: None,
            saving: None,
            close_after_save: None,
            exit_after_save: None,
            exit_after_jobs: false,
            searching: None,
            search_due: None,
            journal: pdfinator::recovery::Journal::disabled(),
            show_recovery: false,
            recovered_draft: None,
            recovering_from: None,
        }
    }
}
impl App {
    pub(super) fn poll_work(&mut self, ctx: &egui::Context) {
        if self.work.opening.is_none()
            && self.work.ready.is_none()
            && let Some(path) = self.work.paths.pop_front()
        {
            let (tx, rx) = mpsc::channel();
            self.work.opening_path = Some(path.clone());
            self.work.opening = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Editor::open(&path));
                ctx.request_repaint();
            });
        }
        if let Some(rx) = &self.work.opening {
            match rx.try_recv() {
                Ok(result) => {
                    self.work.opening_path = None;
                    self.work.opening = None;
                    match result {
                        Ok(e) => self.work.ready = Some(e),
                        Err(e) => self.message = Some(e),
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.work.opening_path = None;
                    self.work.opening = None;
                    self.message = Some("L’ouverture s’est interrompue.".into());
                }
                _ => {}
            }
        }
        if self.work.ready.is_some()
            && self.finish_text()
            && let Some(e) = self.work.ready.take()
        {
            self.install_opened(e);
        }
        if let Some(rx) = &self.work.saving {
            match rx.try_recv() {
                Ok(result) => {
                    self.work.saving = None;
                    match result {
                        Ok(task) => {
                            let mut clean = false;
                            if let Some(e) =
                                self.editor.as_mut().filter(|e| e.instance == task.instance)
                            {
                                e.accept_save(&task);
                                clean = !e.dirty();
                            }
                            for doc in self.documents.iter_mut().flatten() {
                                if let Some(e) =
                                    doc.editor_mut().filter(|e| e.instance == task.instance)
                                {
                                    e.accept_save(&task);
                                    clean = !e.dirty();
                                }
                            }
                            let closing = self.work.close_after_save.take();
                            if clean {
                                self.work.journal.remove(task.instance);
                            }
                            if clean && closing == Some(task.instance) {
                                if let Some(index) = self.document_index(task.instance) {
                                    self.close_document(index);
                                }
                            }
                            if self.work.exit_after_save == Some(task.instance) {
                                self.work.exit_after_save = None;
                                if clean {
                                    self.request_exit(ctx);
                                }
                            }
                        }
                        Err(error) => {
                            self.work.exit_after_jobs = false;
                            self.work.close_after_save = None;
                            self.work.exit_after_save = None;
                            self.message = Some(format!("Enregistrement impossible : {error}"));
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.work.saving = None;
                    self.work.exit_after_jobs = false;
                    self.work.close_after_save = None;
                    self.work.exit_after_save = None;
                    self.message=Some("L’enregistrement s’est interrompu. La copie de récupération est conservée.".into());
                }
                _ => {}
            }
        }
        if let Some(job) = &self.work.searching {
            match job.receiver.try_recv() {
                Ok(result) => {
                    let current = self.search
                        && !job.cancelled.load(std::sync::atomic::Ordering::Acquire)
                        && self.query == job.query
                        && self.editor.as_ref().is_some_and(|e| {
                            e.instance == job.instance && Arc::ptr_eq(&e.bytes, &job.bytes)
                        });
                    self.work.searching = None;
                    if current {
                        match result {
                            Ok((hits, text)) => {
                                self.hits = hits;
                                self.hit = 0;
                                self.text.extend(text);
                                self.jump = self.hits.first().map(|h| h.0);
                            }
                            Err(e) => self.message = Some(e),
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.work.searching = None;
                }
                _ => {}
            }
        }
        if self.work.searching.is_none()
            && self.work.search_due.is_some_and(|at| Instant::now() >= at)
        {
            self.work.search_due = None;
            if self.search
                && !self.query.is_empty()
                && let Some(e) = &self.editor
            {
                let bytes = e.bytes.clone();
                let source = bytes.clone();
                let query = self.query.clone();
                let needle = query.clone();
                let instance = e.instance;
                let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let stop = cancelled.clone();
                let (tx, rx) = mpsc::channel();
                self.work.searching = Some(SearchJob {
                    cancelled,
                    receiver: rx,
                    instance,
                    bytes,
                    query,
                });
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = pdfinator::document::guarded(|| {
                        let e = Editor::from_bytes(source.as_ref().clone())?;
                        let mut hits = Vec::new();
                        let mut text = Vec::new();
                        for page in 0..e.info.pages.len() {
                            if stop.load(std::sync::atomic::Ordering::Acquire) {
                                return Err("Recherche annulée.".into());
                            }
                            let t = e.text(page)?;
                            hits.extend(t.find(&needle).into_iter().map(|r| (page, r)));
                            text.push((page, t));
                        }
                        Ok((hits, text))
                    });
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
            }
        }
        if self.work.opening.is_some()
            || self.work.saving.is_some()
            || self.work.searching.is_some()
            || self.work.search_due.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        if let Some(error) = self.work.journal.error() {
            self.message = Some(error);
        }
    }
    pub(super) fn install_opened(&mut self, e: Editor) {
        if let Some(canonical) = &e.canonical_path {
            let existing = (0..self.documents.len()).find(|&i| {
                let old = if i == self.active_document {
                    self.editor.as_ref()
                } else {
                    self.documents[i].as_ref().and_then(|d| d.editor())
                };
                old.is_some_and(|e| e.canonical_path.as_ref() == Some(canonical))
            });
            if let Some(index) = existing {
                self.activate_document(index);
                return;
            }
        }
        if let Some(old) = self.work.recovering_from.take() {
            self.work.journal.adopt(e.instance, old);
        }
        self.insert_document(e);
        self.objects.clear();
        self.blocks.clear();
        self.text.clear();
        self.selected = None;
        self.inline = None;
        self.drag = None;
        self.page = 0;
        self.page_destination = 1;
        self.page_drag = None;
        self.scroll_offset = Vec2::ZERO;
        self.jump = Some(0);
        self.fit = Fit::Width;
        self.hits.clear();
        self.text_selection = None;
    }
    fn document_index(&self, id: u64) -> Option<usize> {
        (0..self.documents.len()).find(|&i| {
            if i == self.active_document {
                self.editor.as_ref().is_some_and(|e| e.instance == id)
            } else {
                self.documents[i]
                    .as_ref()
                    .and_then(|d| d.editor())
                    .is_some_and(|e| e.instance == id)
            }
        })
    }
    pub(super) fn start_save(&mut self, path: PathBuf) -> bool {
        if self.work.saving.is_some() {
            self.message = Some("Un enregistrement est déjà en cours.".into());
            return false;
        }
        let Some(e) = &self.editor else { return false };
        let task = e.prepare_save(&path);
        let (tx, rx) = mpsc::channel();
        self.work.saving = Some(rx);
        std::thread::spawn(move || {
            let result = task.write().map(|()| task);
            let _ = tx.send(result);
        });
        true
    }
    pub(super) fn observe_recovery(&mut self, ctx: &egui::Context) {
        for doc in self.documents.iter().flatten() {
            doc.checkpoint(&mut self.work.journal);
        }
        if let Some(e) = &self.editor {
            let draft = self
                .inline
                .as_ref()
                .filter(|i| !i.committed && i.changed())
                .map(|i| i.selection.draft.clone())
                .or_else(|| {
                    self.selected
                        .as_ref()
                        .filter(|_| self.comment_changed())
                        .map(|s| s.draft.clone())
                })
                .or_else(|| self.work.recovered_draft.clone())
                .unwrap_or_default();
            let name = e.display_name();
            self.work.journal.observe(
                e.instance,
                e.revision(),
                e.bytes.clone(),
                name,
                draft.clone(),
                e.dirty() || !draft.is_empty(),
            );
        }
        if self.work.journal.active() {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
    }
    pub(super) fn recovery_dialog(&mut self, ctx: &egui::Context) {
        if self.work.help_open {
            let modal = egui::Modal::new(egui::Id::new("help")).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 80.).clamp(320., 560.));
                ui.heading("Outils et raccourcis");
                for (name, description) in [
                    (
                        "Sélectionner",
                        "Glissez un rectangle pour sélectionner plusieurs objets.",
                    ),
                    ("Main", "Glissez la page pour déplacer la vue."),
                    (
                        "Modifier le texte",
                        "Cliquez un texte ; double-cliquez une image pour l’OCR.",
                    ),
                    ("Dessin", "Traits, formes et tableaux ; réglages à droite."),
                    (
                        "Pages",
                        "Clic droit sur une miniature pour ajouter, tourner ou déplacer.",
                    ),
                    (
                        "Ouvrir / Enregistrer / Rechercher",
                        "Ctrl+O / Ctrl+S / Ctrl+F",
                    ),
                    ("Annuler / Rétablir", "Ctrl+Z / Ctrl+Y"),
                ] {
                    ui.strong(name);
                    ui.label(description);
                    ui.add_space(4.);
                }
                if ui.button("Fermer l’aide").clicked() {
                    self.work.help_open = false;
                }
            });
            if modal.should_close() {
                self.work.help_open = false;
            }
        }
        if self.work.show_recovery
            && !self.work.journal.entries.is_empty()
            && self.work.opening.is_none()
            && self.work.ready.is_none()
            && self.work.paths.is_empty()
        {
            let entries = self.work.journal.entries.clone();
            let mut recover = None;
            egui::Modal::new(egui::Id::new("recovery")).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 80.).clamp(320., 560.));
                ui.heading("Récupérer le travail interrompu");
                ui.label("Des copies de travail sont disponibles après une fermeture inattendue.");
                for (i, entry) in entries.iter().enumerate() {
                    if ui
                        .button(format!("Récupérer {}", entry.metadata.name))
                        .clicked()
                    {
                        recover = Some(i);
                    }
                }
                if ui.button("Plus tard").clicked() {
                    self.work.show_recovery = false;
                }
            });
            if let Some(i) = recover {
                let entry = self.work.journal.entries.remove(i);
                self.work.recovering_from = Some(entry.pdf.clone());
                let draft = entry.metadata.draft;
                let name = std::path::Path::new(&entry.metadata.name)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Document récupéré.pdf".into());
                let (tx, rx) = mpsc::channel();
                self.work.opening = Some(rx);
                std::thread::spawn(move || {
                    let result = Editor::open(&entry.pdf).map(|mut e| {
                        e.mark_recovered();
                        e.recovered_name = Some(name);
                        e
                    });
                    let _ = tx.send(result);
                });
                if !draft.is_empty() {
                    self.work.recovered_draft = Some(draft);
                }
                self.work.show_recovery = !self.work.journal.entries.is_empty();
            }
        }
        if let Some(draft) = &mut self.work.recovered_draft {
            let mut close = false;
            egui::Modal::new(egui::Id::new("recovered-draft")).show(ctx,|ui|{
                ui.heading("Brouillon de saisie récupéré");ui.label("Ce texte n’avait pas encore été validé. Copiez-le pour le replacer dans le PDF.");
                ui.add(egui::TextEdit::multiline(draft).desired_width(420.).desired_rows(8));
                if ui.button("Copier le brouillon").clicked(){ctx.copy_text(draft.clone());}
                close=ui.button("J’ai récupéré ce texte").clicked();
            });
            if close {
                self.work.recovered_draft = None;
            }
        }
    }
    pub(super) fn work_status(&mut self, root: &mut egui::Ui) {
        let status = if self.work.exit_after_jobs {
            Some("Fermeture après la fin des opérations…")
        } else if self.work.opening.is_some() || !self.work.paths.is_empty() {
            Some("Ouverture du PDF…")
        } else if self.work.saving.is_some() {
            Some("Enregistrement…")
        } else {
            None
        };
        if let Some(status) = status {
            root.ctx().request_repaint_after(Duration::from_millis(40));
            egui::Panel::bottom("work-status").show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(status);
                    if self.work.exit_after_jobs && ui.button("Annuler la fermeture").clicked() {
                        self.work.exit_after_jobs = false;
                    }
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (egui::Context, App) {
        let ctx = egui::Context::default();
        let app = App::from_context(&ctx, None);
        (ctx, app)
    }
    fn editor(pages: usize) -> Editor {
        let d = pdfcraft_create::blank(595., 842., pages).unwrap();
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap()
    }
    fn wait(ctx: &egui::Context, app: &mut App, ready: impl Fn(&App) -> bool) {
        for _ in 0..1000 {
            app.poll_work(ctx);
            if ready(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("background job did not finish: {:?}", app.message);
    }
    #[test]
    #[cfg(windows)]
    fn reopening_a_case_alias_after_save_as_keeps_the_existing_tab() {
        let (ctx, mut app) = app();
        let dir = tempfile::tempdir_in("test-output").unwrap();
        let first = dir.path().join("original.pdf");
        let saved = dir.path().join("renamed.pdf");
        editor(1).save(&first).unwrap();
        app.install_opened(Editor::open(&first).unwrap());
        let id = app.editor.as_ref().unwrap().instance;
        assert!(app.start_save(saved));
        wait(&ctx, &mut app, |a| a.work.saving.is_none());
        app.open(dir.path().join("RENAMED.PDF"));
        wait(&ctx, &mut app, |a| {
            a.work.opening.is_none() && a.work.paths.is_empty()
        });
        assert_eq!(app.documents.len(), 1);
        assert_eq!(app.editor.as_ref().unwrap().instance, id);
    }
    #[test]
    fn opening_is_queued_and_duplicate_paths_keep_one_session() {
        let (ctx, mut app) = app();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let dir = tempfile::tempdir_in(root).unwrap();
        let path = dir.path().join("test.pdf");
        editor(1).save(&path).unwrap();
        app.open(path.clone());
        app.open(path);
        assert!(app.editor.is_none());
        assert_eq!(app.work.paths.len(), 1);
        wait(&ctx, &mut app, |a| a.editor.is_some());
        assert_eq!(app.documents.len(), 1);
        assert!(app.message.is_none());
    }
    #[test]
    fn saving_an_inactive_tab_never_changes_the_active_documents_identity_or_path() {
        let (ctx, mut app) = app();
        let mut first = editor(1);
        first.insert_blank_after(0).unwrap();
        let id = first.instance;
        app.install_opened(first);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let dir = tempfile::tempdir_in(root).unwrap();
        let path = dir.path().join("premier.pdf");
        assert!(app.start_save(path.clone()));
        let second = editor(3);
        let other = second.instance;
        app.install_opened(second);
        wait(&ctx, &mut app, |a| a.work.saving.is_none());
        assert_eq!(app.editor.as_ref().unwrap().instance, other);
        assert!(app.editor.as_ref().unwrap().path.is_none());
        assert_eq!(Editor::open(&path).unwrap().info.pages.len(), 2);
        let saved = app
            .documents
            .iter()
            .flatten()
            .filter_map(|d| d.editor())
            .find(|e| e.instance == id)
            .unwrap();
        assert!(!saved.dirty());
        assert_eq!(saved.path.as_ref(), Some(&path));
    }
    #[test]
    fn stale_search_results_are_discarded_after_the_query_changes() {
        let (ctx, mut app) = app();
        app.install_opened(editor(4));
        app.search = true;
        app.query = "ancien".into();
        app.find();
        app.work.search_due = Some(Instant::now());
        app.poll_work(&ctx);
        assert!(app.work.searching.is_some());
        app.query = "nouveau".into();
        app.find();
        app.work.search_due = Some(Instant::now());
        wait(&ctx, &mut app, |a| {
            a.work.searching.is_none() && a.work.search_due.is_none()
        });
        assert!(app.hits.is_empty());
        assert!(app.message.is_none());
    }
    #[test]
    fn restarting_the_same_query_does_not_display_the_old_cancellation_as_an_error() {
        let (ctx, mut app) = app();
        app.install_opened(editor(1));
        app.search = true;
        app.query = "meme requete".into();
        let (tx, rx) = mpsc::channel();
        let e = app.editor.as_ref().unwrap();
        app.work.searching = Some(SearchJob {
            receiver: rx,
            instance: e.instance,
            bytes: e.bytes.clone(),
            query: app.query.clone(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        app.find();
        tx.send(Err("Recherche annulée.".into())).unwrap();
        app.poll_work(&ctx);
        assert!(app.message.is_none(), "{:?}", app.message);
    }
}
