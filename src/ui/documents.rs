//! Independent document sessions for tabs; switching never discards unsaved work.
use super::*;
pub(super) struct DocumentState {
    panel_width: f32,
    fit_compensation: f32,
    editor: Option<Editor>,
    raster: raster::Raster,
    objects: HashMap<usize, Vec<EditableObject>>,
    blocks: HashMap<usize, Vec<TextBlock>>,
    text: HashMap<usize, Arc<PageText>>,
    tool: Tool,
    workspace: Workspace,
    selected: Option<Selection>,
    inline: Option<inline::Inline>,
    drag: Option<Drag>,
    zoom: f32,
    fit: Fit,
    page: usize,
    jump: Option<usize>,
    search: bool,
    query: String,
    hits: Vec<(usize, std::ops::Range<usize>)>,
    hit: usize,
    text_selection: Option<(usize, usize, usize)>,
    image_path: Option<PathBuf>,
    thumbnails: bool,
    page_destination: usize,
    page_drag: Option<usize>,
    scroll_offset: Vec2,
}
impl DocumentState {
    fn empty() -> Self {
        Self {
            panel_width: 0.,
            fit_compensation: 0.,
            editor: None,
            raster: Default::default(),
            objects: Default::default(),
            blocks: Default::default(),
            text: Default::default(),
            tool: Tool::Select,
            workspace: Workspace::Read,
            selected: None,
            inline: None,
            drag: None,
            zoom: 1.,
            fit: Fit::Width,
            page: 0,
            jump: Some(0),
            search: false,
            query: String::new(),
            hits: Vec::new(),
            hit: 0,
            text_selection: None,
            image_path: None,
            thumbnails: true,
            page_destination: 1,
            page_drag: None,
            scroll_offset: Vec2::ZERO,
        }
    }
    fn exchange(&mut self, app: &mut App) {
        std::mem::swap(&mut self.panel_width, &mut app.work.panel_width);
        std::mem::swap(&mut self.fit_compensation, &mut app.work.fit_compensation);
        std::mem::swap(&mut self.editor, &mut app.editor);
        std::mem::swap(&mut self.raster, &mut app.raster);
        std::mem::swap(&mut self.objects, &mut app.objects);
        std::mem::swap(&mut self.blocks, &mut app.blocks);
        std::mem::swap(&mut self.text, &mut app.text);
        std::mem::swap(&mut self.tool, &mut app.tool);
        std::mem::swap(&mut self.workspace, &mut app.workspace);
        std::mem::swap(&mut self.selected, &mut app.selected);
        std::mem::swap(&mut self.inline, &mut app.inline);
        std::mem::swap(&mut self.drag, &mut app.drag);
        std::mem::swap(&mut self.zoom, &mut app.zoom);
        std::mem::swap(&mut self.fit, &mut app.fit);
        std::mem::swap(&mut self.page, &mut app.page);
        std::mem::swap(&mut self.jump, &mut app.jump);
        std::mem::swap(&mut self.search, &mut app.search);
        std::mem::swap(&mut self.query, &mut app.query);
        std::mem::swap(&mut self.hits, &mut app.hits);
        std::mem::swap(&mut self.hit, &mut app.hit);
        std::mem::swap(&mut self.text_selection, &mut app.text_selection);
        std::mem::swap(&mut self.image_path, &mut app.image_path);
        std::mem::swap(&mut self.thumbnails, &mut app.thumbnails);
        std::mem::swap(&mut self.page_destination, &mut app.page_destination);
        std::mem::swap(&mut self.page_drag, &mut app.page_drag);
        std::mem::swap(&mut self.scroll_offset, &mut app.scroll_offset);
    }
    fn take(app: &mut App) -> Self {
        let mut state = Self::empty();
        state.exchange(app);
        state
    }
    pub fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.editor.as_mut()
    }
    pub fn dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(Editor::dirty)
            || self
                .inline
                .as_ref()
                .is_some_and(|i| !i.committed && i.changed())
            || self.comment_draft().is_some()
    }
    fn comment_draft(&self) -> Option<&str> {
        let s = self.selected.as_ref()?;
        let Target::Comment(index) = s.target else {
            return None;
        };
        let annotation = self
            .editor
            .as_ref()?
            .info
            .annotations
            .iter()
            .find(|a| a.page == s.page && a.index == index)?;
        (annotation.contents.as_deref().unwrap_or("") != s.draft).then_some(s.draft.as_str())
    }
    pub(super) fn checkpoint(&self, journal: &mut pdfinator::recovery::Journal) {
        let Some(e) = &self.editor else { return };
        let draft = self
            .inline
            .as_ref()
            .filter(|i| !i.committed && i.changed())
            .map(|i| i.selection.draft.as_str())
            .or_else(|| self.comment_draft())
            .unwrap_or("");
        let name = e.display_name();
        journal.observe(
            e.instance,
            e.revision(),
            e.bytes.clone(),
            name,
            draft.to_owned(),
            self.dirty(),
        );
    }
}
impl App {
    pub(super) fn activate_document(&mut self, index: usize) -> bool {
        if index == self.active_document {
            return true;
        }
        if !self.finish_text() {
            return false;
        }
        self.activate_unchecked(index);
        true
    }
    fn activate_unchecked(&mut self, index: usize) {
        if index == self.active_document || index >= self.documents.len() {
            return;
        }
        self.image_crop = None;
        self.image_context = None;
        let Some(mut next) = self.documents.get_mut(index).and_then(Option::take) else {
            return;
        };
        let previous = DocumentState::take(self);
        if let Some(slot) = self.documents.get_mut(self.active_document) {
            *slot = Some(previous);
        }
        next.exchange(self);
        self.active_document = index;
        self.drawing.selected.clear();
        self.drawing.drag = None;
        self.marquee.clear();
        self.message = None;
        self.printing.open = false;
        self.recognition = None;
        self.canvas = Rect::NOTHING;
        self.right_rect = Rect::NOTHING;
    }
    pub(super) fn insert_document(&mut self, editor: Editor) {
        if self.editor.is_some() {
            let previous = DocumentState::take(self);
            if let Some(slot) = self.documents.get_mut(self.active_document) {
                *slot = Some(previous);
            } else {
                self.documents.push(Some(previous));
            }
        }
        self.active_document = self.documents.len();
        self.documents.push(None);
        self.drawing.selected.clear();
        self.drawing.drag = None;
        self.marquee.clear();
        self.raster.document(editor.bytes.clone(), true, None);
        self.editor = Some(editor);
        self.message = None;
        self.printing.open = false;
        self.recognition = None;
    }
    pub(super) fn document_label(&self, index: usize) -> String {
        let editor = if index == self.active_document {
            self.editor.as_ref()
        } else {
            self.documents
                .get(index)
                .and_then(Option::as_ref)
                .and_then(DocumentState::editor)
        };
        editor
            .map(Editor::display_name)
            .unwrap_or_else(|| "Document".into())
    }

    pub(super) fn document_dirty(&self, index: usize) -> bool {
        if index == self.active_document {
            self.comment_changed()
                || self.editor.as_ref().is_some_and(Editor::dirty)
                || self
                    .inline
                    .as_ref()
                    .is_some_and(|i| !i.committed && i.changed())
        } else {
            self.documents
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(DocumentState::dirty)
        }
    }
    pub(super) fn close_document(&mut self, index: usize) {
        if index >= self.documents.len() {
            return;
        }
        if index != self.active_document && !self.activate_document(index) {
            return;
        }
        self.work.close_after_save = None;
        match self.discard_decision() {
            CloseDecision::Proceed => {}
            CloseDecision::WaitForSave(id) => {
                self.work.close_after_save = Some(id);
                return;
            }
            CloseDecision::Cancel => return,
        }
        if let Some(e) = &self.editor {
            self.work.journal.remove(e.instance);
        }
        self.documents.remove(index);
        self.drawing.selected.clear();
        self.drawing.drag = None;
        self.marquee.clear();
        let _discarded = DocumentState::take(self);
        if self.documents.is_empty() {
            self.active_document = 0;
            return;
        }
        self.active_document = index.min(self.documents.len() - 1);
        if let Some(mut next) = self
            .documents
            .get_mut(self.active_document)
            .and_then(Option::take)
        {
            next.exchange(self);
        }
        self.printing.open = false;
        self.recognition = None;
    }
    pub(super) fn discard_all_ok(&mut self) -> bool {
        self.work.exit_after_save = None;
        let original = self.active_document;
        if self.documents.is_empty() {
            return match self.discard_decision() {
                CloseDecision::Proceed => true,
                CloseDecision::WaitForSave(id) => {
                    self.work.exit_after_save = Some(id);
                    false
                }
                CloseDecision::Cancel => false,
            };
        }
        for index in 0..self.documents.len() {
            self.activate_unchecked(index);
            match self.discard_decision() {
                CloseDecision::Proceed => {}
                decision => {
                    if let CloseDecision::WaitForSave(id) = decision {
                        self.work.exit_after_save = Some(id);
                    }
                    self.activate_unchecked(original);
                    return false;
                }
            }
        }
        self.activate_unchecked(original);
        true
    }
    pub(super) fn request_exit(&mut self, ctx: &egui::Context) {
        self.work.exit_after_jobs = false;
        if !self.discard_all_ok() {
            return;
        }
        if self.word_export.is_some() || self.printing.busy || self.work.saving.is_some() {
            self.work.exit_after_jobs = true;
            return;
        }
        self.close_confirmed = true;
        self.work.journal.clean_exit();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
