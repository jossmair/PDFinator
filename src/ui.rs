//! PDFinator's document-first desktop workspace. The PDF engine remains UI-independent.
#![allow(clippy::collapsible_if)]
mod background;
mod canvas;
mod clipboard;
mod comments;
mod crop;
mod documents;
mod drafts;
mod drawing;
mod drawing_style;
mod export;
mod inline;
mod layout;
mod marquee;
mod pages;
mod printing;
mod raster;
mod recognition;
mod tabs;
#[cfg(test)]
mod tests;
mod typography;
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Key, Modifiers, Pos2, Rect, Stroke, StrokeKind, Vec2};
use pdfcraft_edit::{
    AddedText, Align, BlockStyle, Content, EditableObject, Family, ObjectKind, TextBlock,
};
use pdfcraft_render::{PageInfo, PageText};
use pdfinator::document::{Editor, Result};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    Select,
    Hand,
    Move,
    Edit,
    Text,
    Image,
    Highlight,
    Comment,
    Draw,
}
#[derive(Clone, Copy, PartialEq)]
enum Workspace {
    Read,
    Edit,
    Pages,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseDecision {
    Proceed,
    Cancel,
    WaitForSave(u64),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
enum Target {
    Block(usize),
    Added(usize),
    Image(usize),
    Highlight(usize),
    Comment(usize),
    New,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Selection {
    page: usize,
    target: Target,
    rect: [f64; 4],
    draft: String,
    size: f64,
    color: [f32; 3],
    bold: bool,
    #[serde(with = "drafts::family")]
    family: Option<Family>,
    #[serde(with = "drafts::align")]
    align: Option<Align>,
    resource: Option<String>,
    original_size: f64,
    original_color: [f32; 3],
    original_bold: bool,
    is_image: bool,
    #[serde(with = "drafts::family")]
    original_family: Option<Family>,
    #[serde(with = "drafts::align")]
    original_align: Option<Align>,
    line_spacing: Option<f64>,
    original_line_spacing: Option<f64>,
    font_name: String,
    italic: bool,
    rich: Vec<pdfinator::rich::Span>,
    original_rich: Vec<pdfinator::rich::Span>,
    text_range: Option<std::ops::Range<usize>>,
    #[serde(with = "drafts::fonts")]
    font_bindings: Vec<(String, typography::FontFace)>,
    scope_all: bool,
    typing: pdfinator::rich::Format,
}
impl Selection {
    fn text(
        page: usize,
        target: Target,
        rect: [f64; 4],
        text: String,
        size: f64,
        color: [f64; 3],
        bold: bool,
    ) -> Self {
        Self {
            page,
            target,
            rect,
            draft: text,
            size,
            color: color.map(|v| v as f32),
            bold,
            family: None,
            align: None,
            resource: None,
            original_size: size,
            original_color: color.map(|v| v as f32),
            original_bold: bold,
            is_image: false,
            original_family: None,
            original_align: None,
            line_spacing: None,
            original_line_spacing: None,
            font_name: "Helvetica".into(),
            italic: false,
            rich: Vec::new(),
            original_rich: Vec::new(),
            text_range: None,
            font_bindings: Vec::new(),
            scope_all: false,
            typing: Default::default(),
        }
    }
    fn style_changed(&self) -> bool {
        self.size != self.original_size
            || self.color != self.original_color
            || self.bold != self.original_bold
            || self.family != self.original_family
            || self.align != self.original_align
            || self.line_spacing != self.original_line_spacing
            || self.resource.is_some()
            || self.rich != self.original_rich
    }
    fn style(&self) -> BlockStyle {
        BlockStyle {
            resource: self.resource.clone(),
            family: self.family.map(|f| (f, self.bold, false)),
            size: (self.size != self.original_size).then_some(self.size),
            bold: (self.bold != self.original_bold).then_some(self.bold),
            color: (self.color != self.original_color).then_some(self.color.map(f64::from)),
            align: self.align,
            line_spacing: self.line_spacing,
            ..Default::default()
        }
    }
    fn view_rect(&self, p: &PageInfo) -> Rect {
        let [x0, y0, x1, y1] = self.rect;
        if matches!(self.target, Target::Added(_) | Target::New) {
            Rect::from_min_max(
                Pos2::new(x0 as f32, p.height - y1 as f32),
                Pos2::new(x1 as f32, p.height - y0 as f32),
            )
        } else {
            let a = p.user_to_view(x0 as f32, y0 as f32);
            let b = p.user_to_view(x1 as f32, y1 as f32);
            Rect::from_two_pos(Pos2::new(a[0], a[1]), Pos2::new(b[0], b[1]))
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Manual,
    Width,
    Page,
}
#[derive(Clone, Copy, Debug)]
enum DragMode {
    Move,
    Corner(usize),
    Create,
}
struct Drag {
    selection: Selection,
    mode: DragMode,
    start: Pos2,
    source: Rect,
    current: Rect,
    background: raster::Background,
    texture: Option<egui::TextureHandle>,
    page_size: Vec2,
    committed: bool,
    text_preview: Option<inline::Inline>,
}
pub struct App {
    work: background::Work,
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
    canvas: Rect,
    document_width: f32,
    right_rect: Rect,
    message: Option<String>,
    search: bool,
    query: String,
    hits: Vec<(usize, std::ops::Range<usize>)>,
    hit: usize,
    text_selection: Option<(usize, usize, usize)>,
    image_path: Option<PathBuf>,
    close_confirmed: bool,
    animations: bool,
    thumbnails: bool,
    lock_aspect: bool,
    printing: printing::Printing,
    recognition: Option<recognition::Session>,
    thumbnail_visible: Vec<usize>,
    page_destination: usize,
    page_drag: Option<usize>,
    page_selection: pages::PageSelection,
    scroll_offset: Vec2,
    documents: Vec<Option<documents::DocumentState>>,
    active_document: usize,
    fonts: typography::FontBook,
    copied_block: Option<pdfinator::transfer::CopiedText>,
    tab_drag: Option<usize>,
    properties_collapsed: bool,
    image_crop: Option<crop::State>,
    image_context: Option<Selection>,
    copied_image: Option<pdfinator::transfer::CopiedImage>,
    copied_image_sequence: Option<u32>,
    copied_image_frame: u64,
    drawing: drawing::State,
    marquee: marquee::State,
    word_export: Option<export::Job>,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        Self::from_context(&cc.egui_ctx, path)
    }
    fn from_context(ctx: &egui::Context, path: Option<PathBuf>) -> Self {
        Self::configure(ctx);
        let mut app = Self {
            work: Default::default(),
            editor: None,
            raster: Default::default(),
            objects: HashMap::new(),
            blocks: HashMap::new(),
            text: HashMap::new(),
            tool: Tool::Select,
            workspace: Workspace::Read,
            selected: None,
            inline: None,
            drag: None,
            zoom: 1.,
            fit: Fit::Width,
            page: 0,
            jump: None,
            canvas: Rect::NOTHING,
            document_width: 0.,
            right_rect: Rect::NOTHING,
            message: None,
            search: false,
            query: String::new(),
            hits: Vec::new(),
            hit: 0,
            text_selection: None,
            image_path: None,
            close_confirmed: false,
            animations: windows_animations(),
            thumbnails: true,
            lock_aspect: true,
            printing: Default::default(),
            recognition: None,
            thumbnail_visible: Vec::new(),
            page_destination: 1,
            page_drag: None,
            page_selection: Default::default(),
            scroll_offset: Vec2::ZERO,
            documents: Vec::new(),
            active_document: 0,
            fonts: typography::FontBook::start(),
            copied_block: None,
            tab_drag: None,
            properties_collapsed: false,
            image_crop: None,
            image_context: None,
            copied_image: None,
            copied_image_sequence: None,
            copied_image_frame: 0,
            drawing: Default::default(),
            marquee: Default::default(),
            word_export: None,
        };
        #[cfg(not(test))]
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            match pdfinator::recovery::Journal::start(
                &PathBuf::from(local).join("Ergonatus/PDFinator/Recovery"),
            ) {
                Ok(journal) => {
                    app.work.show_recovery = !journal.entries.is_empty();
                    app.work.journal = journal;
                }
                Err(e) => {
                    app.message = Some(format!(
                        "La récupération automatique est indisponible : {e}"
                    ))
                }
            }
        }
        if let Some(p) = path {
            app.open(p);
        }
        app
    }
    fn configure(ctx: &egui::Context) {
        egui_extras::install_image_loaders(ctx);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        ctx.set_visuals(egui::Visuals::light());
        let mut fonts = egui::FontDefinitions::default();
        for (name, bytes) in [
            (
                "Interface",
                include_bytes!("../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf").as_slice(),
            ),
            (
                "Interface-Bold",
                include_bytes!("../vendor/pdfcraft/assets/fonts/Inter-SemiBold.ttf").as_slice(),
            ),
        ] {
            fonts
                .font_data
                .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
            fonts
                .families
                .insert(egui::FontFamily::Name(name.into()), vec![name.into()]);
        }
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Interface".into());
        ctx.set_fonts(fonts);
        let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
        style.spacing.item_spacing = Vec2::new(6., 6.);
        style.spacing.interact_size = Vec2::new(30., 30.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.));
        style.visuals.override_text_color = Some(icons::INK);
        style.visuals.panel_fill = Color32::from_rgb(249, 251, 254);
        style.visuals.selection.bg_fill = Color32::from_rgb(198, 215, 255);
        style.visuals.selection.stroke = Stroke::new(1., icons::BLUE);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(241, 245, 251);
        style.visuals.widgets.inactive.bg_stroke =
            Stroke::new(1., Color32::from_rgb(220, 228, 240));
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(226, 235, 255);
        for v in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            v.corner_radius = egui::CornerRadius::same(5);
        }
        ctx.set_style_of(egui::Theme::Light, style);
    }
    fn refresh(&mut self, page: usize) {
        if let Some(e) = &self.editor {
            self.raster.document(e.bytes.clone(), false, Some(page));
        }
        self.objects.remove(&page);
        self.blocks.remove(&page);
        self.text.remove(&page);
        self.hits.clear();
        self.text_selection = None;
    }
    fn changed(&mut self, result: Result<()>, page: usize) -> bool {
        match result {
            Ok(()) => {
                self.refresh(page);
                true
            }
            Err(error) => {
                self.message = Some(error);
                false
            }
        }
    }
    fn permit(&mut self) -> bool {
        let Some(e) = &mut self.editor else {
            return false;
        };
        if e.signed && !e.signature_accepted {
            e.signature_accepted=rfd::MessageDialog::new().set_title("Signature numérique").set_description("Modifier ce PDF peut invalider sa signature numérique. Continuer ? Conservez l’original avec Enregistrer sous.").set_buttons(rfd::MessageButtons::YesNo).show()==rfd::MessageDialogResult::Yes;
            return e.signature_accepted;
        }
        true
    }
    fn discard_decision(&mut self) -> CloseDecision {
        if !self.editor.as_ref().is_some_and(Editor::dirty)
            && !self
                .inline
                .as_ref()
                .is_some_and(|i| !i.committed && i.changed())
            && !self.comment_changed()
        {
            return CloseDecision::Proceed;
        }
        let choice = rfd::MessageDialog::new()
            .set_title("Modifications non enregistrées")
            .set_description("Enregistrer avant de continuer ?")
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        self.resolve_discard_choice(choice)
    }
    fn resolve_discard_choice(&mut self, choice: rfd::MessageDialogResult) -> CloseDecision {
        match choice {
            rfd::MessageDialogResult::Yes => {
                if self.save(false) {
                    CloseDecision::WaitForSave(self.editor.as_ref().unwrap().instance)
                } else {
                    CloseDecision::Cancel
                }
            }
            rfd::MessageDialogResult::No => CloseDecision::Proceed,
            _ => CloseDecision::Cancel,
        }
    }
    fn open(&mut self, path: PathBuf) {
        if self.work.paths.contains(&path) || self.work.opening_path.as_ref() == Some(&path) {
            return;
        }
        if let Some(index) = (0..self.documents.len()).find(|&index| {
            let e = if index == self.active_document {
                self.editor.as_ref()
            } else {
                self.documents
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(documents::DocumentState::editor)
            };
            e.and_then(|e| e.path.as_ref()).is_some_and(|p| p == &path)
        }) {
            self.activate_document(index);
            return;
        }
        if !self.finish_text() {
            return;
        }
        self.work.paths.push_back(path);
    }
    fn pick_open(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Documents PDF", &["pdf"])
            .pick_files()
        {
            for path in paths {
                self.open(path);
            }
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        if !self.finish_text() {
            return false;
        }
        let Some(e) = &mut self.editor else {
            return false;
        };
        let path = if save_as || e.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Document PDF", &["pdf"])
                .set_file_name(
                    e.path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .or_else(|| e.recovered_name.clone())
                        .unwrap_or_else(|| "document-modifie.pdf".into()),
                )
                .save_file()
        } else {
            e.path.clone()
        };
        let Some(path) = path else { return false };
        self.start_save(path)
    }
    fn save_keep_focus(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        if self.inline.as_ref().is_none_or(|i| i.committed) {
            return self.save(save_as);
        }
        if !self.commit_text_keep_focus(ctx) {
            return false;
        }
        let mut inline = self.inline.take();
        let saved = self.save(save_as);
        if let Some(i) = &mut inline {
            i.focus = true;
        }
        self.inline = inline;
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("direct-text")));
        saved
    }
    fn deselect(&mut self) -> bool {
        if !self.finish_text() {
            return false;
        }
        self.image_crop = None;
        self.image_context = None;
        self.selected = None;
        self.marquee.clear();
        self.drawing.selected.clear();
        self.drawing.drag = None;
        self.drag = None;
        self.text_selection = None;
        self.close_search();
        true
    }
    fn set_workspace(&mut self, ctx: &egui::Context, workspace: Workspace) {
        let tool = if workspace == Workspace::Edit {
            Tool::Edit
        } else {
            Tool::Select
        };
        if self.set_tool(ctx, tool) {
            self.workspace = workspace;
            self.page_destination = self.page + 1;
        }
    }
    fn set_tool(&mut self, ctx: &egui::Context, tool: Tool) -> bool {
        if !self.deselect() {
            return false;
        }
        self.tool = tool;
        self.image_crop = None;
        self.page_drag = None;
        self.printing.open = false;
        if tool == Tool::Image {
            self.image_path = rfd::FileDialog::new()
                .add_filter("Images PNG et JPEG", &["png", "jpg", "jpeg"])
                .pick_file();
            if self.image_path.is_none() {
                self.tool = Tool::Select;
            }
        }
        ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("direct-text")));
        true
    }
    fn begin_text(&mut self, ctx: &egui::Context, selection: Selection) {
        if selection.is_image
            || matches!(selection.target, Target::Highlight(_) | Target::Comment(_))
        {
            self.selected = Some(selection);
            return;
        }
        if !self.finish_text() || !self.permit() {
            return;
        }
        let Some(e) = &self.editor else { return };
        if let Target::Block(i) = selection.target {
            if e.blocks(selection.page)
                .ok()
                .and_then(|b| b.get(i).cloned())
                .is_some_and(|b| b.invisible)
            {
                self.message=Some("La couche OCR est invisible. PDFinator ne recouvre pas le scan ; ajoutez du texte dans une zone libre.".into());
                return;
            }
        }
        match inline::Inline::new(
            ctx,
            e,
            selection.clone(),
            (self.zoom * ctx.pixels_per_point()).clamp(0.25, 4.),
        ) {
            Ok(inline) => {
                self.selected = Some(selection);
                self.inline = Some(inline);
            }
            Err(e) => self.message = Some(e),
        }
    }
    fn finish_text(&mut self) -> bool {
        if !self.finish_comment() {
            return false;
        }
        let Some(mut inline) = self.inline.take() else {
            return true;
        };
        if inline.committed {
            self.inline = Some(inline);
            return true;
        }
        if matches!(inline.selection.target, Target::New) && inline.selection.draft.is_empty() {
            self.selected = Some(inline.selection);
            return true;
        }
        if !inline.changed() {
            self.selected = Some(inline.selection);
            return true;
        }
        let s = inline.selection.clone();
        if !self.permit() {
            self.inline = Some(inline);
            return false;
        }
        let (target, result) = match self.commit_text_selection(&s) {
            Ok(target) => (target, Ok(())),
            Err(error) => (s.target, Err(error)),
        };
        if self.changed(result, s.page) {
            inline.committed = true;
            inline.selection.target = target;
            self.selected = self
                .selection_for(s.page, target)
                .or_else(|| Some(inline.selection.clone()));
            self.inline = Some(inline);
            true
        } else {
            self.inline = Some(inline);
            false
        }
    }
    /// Enter/Apply commit without replacing the user's caret with a toolbar focus.
    /// Outside clicks and workspace changes still use finish_text to leave editing.
    fn commit_text_keep_focus(&mut self, ctx: &egui::Context) -> bool {
        if self.inline.as_ref().is_none_or(|i| i.committed) {
            return self.finish_text();
        }
        let state = egui::TextEdit::load_state(ctx, egui::Id::new("direct-text"));
        let texture = self
            .inline
            .as_ref()
            .and_then(|i| i.background.texture.clone());
        let view = (self.scroll_offset, self.zoom, self.fit, self.page);
        if !self.finish_text() {
            return false;
        }
        let Some(selection) = self.selected.clone() else {
            return true;
        };
        let Some(editor) = &self.editor else {
            return true;
        };
        match inline::Inline::new(
            ctx,
            editor,
            selection,
            (self.zoom * ctx.pixels_per_point()).clamp(0.25, 4.),
        ) {
            Ok(mut inline) => {
                inline.background.texture = texture;
                self.inline = Some(inline);
                if let Some(state) = state {
                    egui::TextEdit::store_state(ctx, egui::Id::new("direct-text"), state);
                }
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("direct-text")));
                (self.scroll_offset, self.zoom, self.fit, self.page) = view;
                true
            }
            Err(error) => {
                self.message = Some(error);
                false
            }
        }
    }
    fn undo_redo(&mut self, redo: bool) {
        self.image_crop = None;
        if !self.finish_text() {
            return;
        }
        if let Some(e) = &mut self.editor {
            let previous_count = e.info.pages.len();
            let result = if redo { e.redo() } else { e.undo() };
            if self.changed(result, self.page) {
                self.inline = None;
                self.selected = None;
                self.marquee.clear();
                self.drawing.selected.clear();
                self.drawing.drag = None;
                self.objects.clear();
                self.blocks.clear();
                self.text.clear();
                self.hits.clear();
                self.text_selection = None;
                if let Some(e) = &self.editor {
                    let changed_count = e.info.pages.len() != previous_count;
                    self.page = self.page.min(e.info.pages.len().saturating_sub(1));
                    self.page_selection.replace([self.page]);
                    self.page_destination = self.page + 1;
                    if changed_count {
                        self.jump = Some(self.page);
                        self.page_drag = None;
                    }
                    self.raster.document(e.bytes.clone(), changed_count, None);
                }
            }
        }
    }
    fn selection_for(&self, page: usize, target: Target) -> Option<Selection> {
        let e = self.editor.as_ref()?;
        match target {
            Target::Block(i) => {
                let b = e.blocks(page).ok()?.get(i)?.clone();
                let mut s =
                    Selection::text(page, target, b.rect, b.editor_text, b.size, b.color, b.bold);
                s.font_name = b
                    .base_font
                    .rsplit('+')
                    .next()
                    .unwrap_or(&b.base_font)
                    .to_string();
                s.italic = b.italic;
                if let Some(data) = e.text_formatting(page, i) {
                    s.rich = data.spans;
                    s.original_rich = s.rich.clone();
                    s.font_name = data.font_name;
                    s.size = data.base_size;
                    s.original_size = s.size;
                    s.color = data.base_color.map(|v| v as f32);
                    s.original_color = s.color;
                    s.bold = data.base_bold;
                    s.original_bold = s.bold;
                    s.italic = data.base_italic;
                    s.align = data.align.as_deref().map(|a| match a {
                        "Centre" => Align::Center,
                        "Droite" => Align::Right,
                        "Justifié" => Align::Justify,
                        _ => Align::Left,
                    });
                    s.original_align = s.align;
                    s.line_spacing = data.line_spacing;
                    s.original_line_spacing = s.line_spacing;
                }
                Some(s)
            }
            Target::Added(i) => {
                let c = e.added(page).get(i)?.clone();
                Some(match c {
                    Content::Text(t) => {
                        let mut s =
                            Selection::text(page, target, t.rect, t.text, t.size, t.color, t.bold);
                        s.family = Some(t.family);
                        s.font_name = t.family.base_font(t.bold, t.italic).into();
                        s.italic = t.italic;
                        s.align = Some(t.align);
                        s.original_family = s.family;
                        s.original_align = s.align;
                        s
                    }
                    Content::Image(t) => {
                        let mut s = Selection::text(
                            page,
                            target,
                            t.rect,
                            String::new(),
                            12.,
                            [0.; 3],
                            false,
                        );
                        s.is_image = true;
                        s
                    }
                })
            }
            Target::Image(i) => {
                let image = pdfcraft_edit::page_images(&e.doc, page)
                    .ok()?
                    .get(i)?
                    .clone();
                let mut s = Selection::text(
                    page,
                    target,
                    image.visible_rect.unwrap_or(image.rect),
                    String::new(),
                    12.,
                    [0.; 3],
                    false,
                );
                s.is_image = true;
                Some(s)
            }
            Target::Highlight(i) | Target::Comment(i) => {
                let a = e
                    .info
                    .annotations
                    .iter()
                    .find(|a| a.page == page && a.index == i)?;
                Some(Selection::text(
                    page,
                    target,
                    a.rect.map(f64::from),
                    if matches!(target, Target::Comment(_)) {
                        a.contents.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                    12.,
                    a.color.unwrap_or([1., 0.94, 0.]).map(f64::from),
                    false,
                ))
            }
            Target::New => None,
        }
    }
    fn delete(&mut self) {
        if !self.permit() {
            return;
        }
        let Some(s) = self.selected.clone() else {
            return;
        };
        let Some(e) = &mut self.editor else { return };
        let result = match s.target {
            Target::Added(i) => e.delete_added(s.page, i),
            Target::Highlight(i) | Target::Comment(i) => e.delete_highlight(s.page, i),
            Target::Image(i) => e.change(false, |d| {
                pdfcraft_edit::change_image(
                    d,
                    s.page,
                    i,
                    &pdfcraft_edit::images::ImageChange::Delete,
                )
                .map_err(|e| e.to_string())
            }),
            Target::Block(i) => e.delete_text_block(s.page, i),
            Target::New => {
                self.selected = None;
                self.inline = None;
                self.drag = None;
                return;
            }
        };
        if self.changed(result, s.page) {
            self.image_crop = None;
            self.image_context = None;
            self.marquee.clear();
            self.selected = None;
            self.inline = None;
            self.drag = None;
        }
    }
    fn move_selected(&mut self, delta: Vec2) {
        if !self.finish_text() || !self.permit() {
            return;
        }
        let Some(s) = self.selected.clone() else {
            return;
        };
        let Some(e) = &self.editor else { return };
        let Some(p) = e.info.pages.get(s.page) else {
            return;
        };
        let r = s.view_rect(p).translate(delta);
        self.transform(s, r);
    }
    fn transform(&mut self, s: Selection, view: Rect) {
        let Some(e) = &mut self.editor else { return };
        let Some(p) = e.info.pages.get(s.page).cloned() else {
            return;
        };
        if matches!(s.target, Target::New) {
            let mut selected = s;
            selected.rect = [
                view.left() as f64,
                (p.height - view.bottom()) as f64,
                view.right() as f64,
                (p.height - view.top()) as f64,
            ];
            self.selected = Some(selected);
            self.inline = None;
            return;
        }
        let old = s.view_rect(&p);
        let resizing = (old.size() - view.size()).length() > 0.05;
        let user = p
            .view_rect_to_user([view.left(), view.top(), view.right(), view.bottom()])
            .map(f64::from);
        let result = match s.target {
            Target::Block(i) => {
                if resizing {
                    let style = BlockStyle {
                        offset: Some([user[0] - s.rect[0], user[3] - s.rect[3]]),
                        width: Some((user[2] - user[0]).max(4.)),
                        ..Default::default()
                    };
                    e.rewrite(s.page, i, None, &style)
                } else {
                    let a = p.view_to_user(old.left(), old.top());
                    let b = p.view_to_user(view.left(), view.top());
                    e.move_block(s.page, i, [(b[0] - a[0]) as f64, (b[1] - a[1]) as f64])
                }
            }
            Target::Added(i) => {
                let rect = [
                    view.left() as f64,
                    (p.height - view.bottom()) as f64,
                    view.right() as f64,
                    (p.height - view.top()) as f64,
                ];
                match e.added(s.page).get(i).cloned() {
                    Some(c) => e.update_added(s.page, i, c.with_rect(rect)),
                    None => Err("Élément introuvable.".into()),
                }
            }
            Target::Image(i) => e.change(false, |d| {
                pdfcraft_edit::change_image(
                    d,
                    s.page,
                    i,
                    &pdfcraft_edit::ImageChange::Transform(pdfcraft_edit::rect_to_rect(
                        s.rect, user,
                    )),
                )
                .map_err(|e| e.to_string())
            }),
            _ => return,
        };
        if self.changed(result, s.page) {
            self.inline = None;
            self.selected = self.selection_for(s.page, s.target);
        }
    }
    fn find(&mut self) {
        if let Some(job) = &self.work.searching {
            job.cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.hits.clear();
        self.hit = 0;
        self.work.search_due = None;
        if self.query.is_empty() {
            return;
        }
        if let Some(e) = &self.editor {
            if e.info.pages.len() <= 3
                && (0..e.info.pages.len()).all(|p| self.text.contains_key(&p))
            {
                for page in 0..e.info.pages.len() {
                    if let Some(t) = self.text.get(&page) {
                        self.hits
                            .extend(t.find(&self.query).into_iter().map(|r| (page, r)));
                    }
                }
                self.jump = self.hits.first().map(|h| h.0);
            } else {
                self.work.search_due =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
            }
        }
    }
    fn close_search(&mut self) {
        if let Some(job) = &self.work.searching {
            job.cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.work.search_due = None;
        self.search = false;
        self.hits.clear();
        self.hit = 0;
    }
    fn request_print(&mut self) {
        if !self.deselect() {
            return;
        }
        if self
            .editor
            .as_ref()
            .is_some_and(|e| e.doc.permissions().is_some_and(|p| !p.print()))
        {
            self.message = Some("Les permissions de ce PDF interdisent l’impression.".into());
            return;
        }
        if self.editor.is_some() {
            if self.printing.busy {
                self.printing.open = true;
            } else {
                self.printing.load();
            }
        }
    }
    fn request_search(&mut self, ctx: &egui::Context) {
        if !self.finish_text() {
            return;
        }
        self.search = true;
        self.printing.open = false;
        self.workspace = Workspace::Read;
        self.tool = Tool::Select;
        self.selected = None;
        self.text_selection = None;
        self.find();
        self.jump = Some(self.page);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("find")));
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.work.help_open = !self.work.help_open;
        }
        if self.work.help_open || self.work.show_recovery || self.work.recovered_draft.is_some() {
            return;
        }
        if self.printing.open || self.recognition.is_some() {
            return;
        }
        let ctrl_shift = Modifiers {
            shift: true,
            ..Modifiers::CTRL
        };
        if self.workspace == Workspace::Pages
            && ctx.input_mut(|i| i.consume_key(ctrl_shift, Key::O))
        {
            self.pick_append_pdfs();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_shift, Key::S)) {
            self.save_keep_focus(ctx, true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::W)) {
            self.close_document(self.active_document);
            return;
        }
        let previous = ctx.input_mut(|i| i.consume_key(ctrl_shift, Key::Tab));
        let next = ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Tab));
        if (previous || next) && self.documents.len() > 1 {
            let count = self.documents.len();
            self.activate_document(if previous {
                (self.active_document + count - 1) % count
            } else {
                (self.active_document + 1) % count
            });
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
            self.pick_open();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S)) {
            self.save_keep_focus(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::P)) {
            self.request_print();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            self.request_search(ctx);
        }
        if self.search && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.close_search();
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("find")));
        }
        if self.block_clipboard_shortcuts(ctx) {
            return;
        }
        if self.inline.as_ref().is_some_and(|i| !i.committed) {
            if egui::Popup::is_any_open(ctx)
                || (ctx.egui_wants_keyboard_input()
                    && !ctx.memory(|m| m.has_focus(egui::Id::new("direct-text"))))
            {
                return;
            }
            for (key, style) in [(Key::B, 0), (Key::I, 1), (Key::U, 2)] {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, key)) {
                    if let Some(s) = self.inline.as_ref().map(|i| i.selection.clone()) {
                        let f = s.active_format();
                        match style {
                            0 => self.font_variant(
                                ctx,
                                !f.bold.unwrap_or(s.bold),
                                f.italic.unwrap_or(s.italic),
                            ),
                            1 => self.font_variant(
                                ctx,
                                f.bold.unwrap_or(s.bold),
                                !f.italic.unwrap_or(s.italic),
                            ),
                            _ => self.apply_format(
                                ctx,
                                pdfinator::rich::Format {
                                    underline: Some(!f.underline.unwrap_or(false)),
                                    ..Default::default()
                                },
                                None,
                            ),
                        }
                    }
                }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.inline = None;
            } else if ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::CTRL, Key::Enter)
            }) {
                self.commit_text_keep_focus(ctx);
            }
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if self.workspace == Workspace::Pages {
            self.page_selection_shortcuts(ctx);
            let count = self.editor.as_ref().map_or(0, |e| e.info.pages.len());
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                self.open_page(self.page);
                return;
            }
            for (key, direction) in [(Key::ArrowLeft, -1_isize), (Key::ArrowRight, 1)] {
                let dest = self
                    .page
                    .saturating_add_signed(direction)
                    .min(count.saturating_sub(1));
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, key)) {
                    self.move_current_page(dest);
                } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    self.page_selection.replace([dest]);
                    self.page = dest;
                    self.page_destination = dest + 1;
                    self.jump = Some(dest);
                }
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            self.undo_redo(false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)) {
            self.undo_redo(true);
        }
        if self.mixed_shortcuts(ctx) {
            return;
        }
        if self.tool == Tool::Draw || !self.drawing.selected.is_empty() {
            self.drawing_shortcuts(ctx);
            return;
        }
        if self.workspace == Workspace::Pages {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::PageDown)) {
            self.jump = Some(
                self.page.saturating_add(1).min(
                    self.editor
                        .as_ref()
                        .map_or(0, |e| e.info.pages.len().saturating_sub(1)),
                ),
            );
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::PageUp)) {
            self.jump = Some(self.page.saturating_sub(1));
        }
        if ctx.input_mut(|i| {
            let key = i.consume_key(Modifiers::CTRL, Key::C);
            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            if key || copy {
                i.events.retain(|e| !matches!(e, egui::Event::Copy));
            }
            key || copy
        }) && self.editor.as_ref().is_some_and(Editor::can_copy)
        {
            if let Some((p, a, b)) = self.text_selection {
                if let Some(t) = self.text.get(&p) {
                    ctx.copy_text(t.text_of(a.min(b)..a.max(b) + 1));
                }
            } else if let Some(s) = &self.selected {
                ctx.copy_text(s.draft.clone());
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.delete();
        }
        if ctx.input(|i| i.key_pressed(Key::F2)) {
            if let Some(s) = self.selected.clone() {
                self.begin_text(ctx, s);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F6)) {
            self.select_next(ctx);
        }
        let step = if ctx.input(|i| i.modifiers.shift) {
            10.
        } else {
            1.
        };
        for (key, delta) in [
            (Key::ArrowLeft, Vec2::new(-step, 0.)),
            (Key::ArrowRight, Vec2::new(step, 0.)),
            (Key::ArrowUp, Vec2::new(0., -step)),
            (Key::ArrowDown, Vec2::new(0., step)),
        ] {
            if ctx.input(|i| i.key_pressed(key)) {
                self.move_selected(delta);
            }
        }
    }
}
fn windows_animations() -> bool {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Control Panel\\Desktop\\WindowMetrics")
            .and_then(|k| k.get_value::<String, _>("MinAnimate"))
            .map(|v| v != "0")
            .unwrap_or(true)
    }
    #[cfg(not(windows))]
    true
}

fn moving_cursor(_ctx: &egui::Context) -> egui::CursorIcon {
    egui::CursorIcon::Move
}
