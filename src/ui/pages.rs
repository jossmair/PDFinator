//! Page overview and navigation. All mutations use the same transactional history as text.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct PageDropTarget {
    pub destination: usize,
    pub marker: [Pos2; 2],
}
impl PageDropTarget {
    pub(super) fn closest(
        source: usize,
        pos: Pos2,
        cards: &[(usize, Rect)],
        vertical: bool,
    ) -> Option<Self> {
        let &(page, card) = cards.iter().min_by(|(_, a), (_, b)| {
            a.distance_sq_to_pos(pos)
                .total_cmp(&b.distance_sq_to_pos(pos))
        })?;
        let after = if vertical {
            pos.y >= card.center().y
        } else {
            pos.x >= card.center().x
        };
        let slot = page + usize::from(after);
        let marker = if vertical {
            let y = if after {
                card.bottom() + 5.
            } else {
                card.top() - 5.
            };
            [
                Pos2::new(card.left() + 4., y),
                Pos2::new(card.right() - 4., y),
            ]
        } else {
            let x = if after {
                card.right() + 7.
            } else {
                card.left() - 7.
            };
            [
                Pos2::new(x, card.top() + 4.),
                Pos2::new(x, card.bottom() - 4.),
            ]
        };
        Some(Self {
            destination: slot.saturating_sub(usize::from(source < slot)),
            marker,
        })
    }
}

impl App {
    pub(super) fn page_context_menu(&mut self, response: &egui::Response, page: usize) {
        response.context_menu(|ui| {
            let count = self.editor.as_ref().map_or(0, |e| e.info.pages.len());
            let mut action = None;
            for (id, label) in [
                "Ajouter une page blanche",
                "Dupliquer",
                "Supprimer",
                "Tourner à gauche",
                "Tourner à droite",
            ]
            .iter()
            .enumerate()
            {
                if ui
                    .add_enabled(id != 2 || count > 1, egui::Button::new(*label))
                    .clicked()
                {
                    action = Some(id);
                    ui.close();
                }
            }
            ui.separator();
            let mut destination = page + 1;
            let key = egui::Id::new(("page-menu-position", page));
            destination = ui.ctx().data_mut(|d| *d.get_temp_mut_or(key, destination));
            ui.horizontal(|ui| {
                ui.label("Déplacer à la page");
                ui.add(egui::DragValue::new(&mut destination).range(1..=count.max(1)));
            });
            ui.ctx().data_mut(|d| d.insert_temp(key, destination));
            if ui.button("Déplacer").clicked() {
                self.page = page;
                self.move_current_page(destination.saturating_sub(1));
                ui.close();
            }
            if let Some(action) = action {
                if !self.finish_text() || !self.permit() {
                    return;
                }
                self.page = page;
                if let Some(e) = &mut self.editor {
                    let result = match action {
                        0 => e.insert_blank_after(page).map(|_| ()),
                        1 => e.duplicate_page(page),
                        2 => e.delete_page(page),
                        3 => e.rotate_page(page, -90),
                        _ => e.rotate_page(page, 90),
                    };
                    match result {
                        Ok(()) => self.reset_page_views(true),
                        Err(e) => self.message = Some(e),
                    }
                }
            }
        });
    }
    pub(super) fn page_drag_input(&mut self, ctx: &egui::Context) {
        let released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        if (self.page_drag.is_some()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)))
            || (!released && !ctx.input(|i| i.pointer.primary_down()))
        {
            self.page_drag = None;
        }
    }
    pub(super) fn begin_page_drag(&mut self, page: usize) {
        if self.deselect() {
            self.page = page;
            self.page_destination = page + 1;
            self.page_drag = Some(page);
        }
    }
    fn reset_page_views(&mut self, reset_raster: bool) {
        self.selected = None;
        self.inline = None;
        self.drag = None;
        self.objects.clear();
        self.blocks.clear();
        self.text.clear();
        self.hits.clear();
        self.text_selection = None;
        self.drawing.selected.clear();
        self.marquee.clear();
        self.drawing.drag = None;
        if let Some(e) = &self.editor {
            self.page = self.page.min(e.info.pages.len().saturating_sub(1));
            self.raster.document(e.bytes.clone(), reset_raster, None);
        }
    }
    pub(super) fn insert_blank_page(&mut self) {
        if self.workspace != Workspace::Edit || !self.deselect() || !self.permit() {
            return;
        }
        let Some(editor) = &mut self.editor else {
            return;
        };
        match editor.insert_blank_after(self.page) {
            Ok(page) => {
                self.page = page;
                self.page_destination = page + 1;
                self.reset_page_views(true);
                self.jump = Some(page);
                self.message = None;
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn rotate_current_page(&mut self, degrees: i64) {
        if !self.finish_text() || !self.permit() {
            return;
        }
        if let Some(e) = &mut self.editor {
            match e.rotate_page(self.page, degrees) {
                Ok(()) => self.reset_page_views(true),
                Err(error) => self.message = Some(error),
            }
        }
    }
    pub(super) fn move_current_page(&mut self, destination: usize) {
        if destination == self.page || !self.finish_text() || !self.permit() {
            return;
        }
        if let Some(e) = &mut self.editor {
            match e.move_page(self.page, destination) {
                Ok(()) => {
                    self.page = destination;
                    self.page_destination = destination + 1;
                    self.reset_page_views(true);
                }
                Err(error) => self.message = Some(error),
            }
        }
    }
    pub(super) fn open_page(&mut self, page: usize) {
        if !self.finish_text() {
            return;
        }
        self.page = page;
        self.workspace = Workspace::Read;
        self.tool = Tool::Select;
        self.selected = None;
        self.jump = Some(page);
    }
    pub(super) fn page_controls(&mut self, ui: &mut egui::Ui, cell: f32) {
        let count = self.editor.as_ref().map_or(0, |e| e.info.pages.len());
        let action = |ui: &mut egui::Ui, icon, label: &str, enabled| {
            icons::button(ui, icon, label, false, enabled, cell)
                .on_hover_text(label)
                .clicked()
        };
        if action(
            ui,
            Icon::AddPdf,
            "Ajouter des PDF à la fin · Ctrl+Maj+O",
            count > 0,
        ) {
            self.pick_append_pdfs();
        }
        ui.horizontal(|ui| {
            if action(ui, Icon::RotateLeft, "Tourner à gauche · 90°", count > 0) {
                self.rotate_current_page(-90);
            }
            if action(ui, Icon::RotateRight, "Tourner à droite · 90°", count > 0) {
                self.rotate_current_page(90);
            }
        });
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(format!("{} / {count}", self.page + 1))
                    .color(Color32::WHITE)
                    .strong()
                    .size(12.),
            );
        });
        ui.horizontal(|ui| {
            if action(ui, Icon::Previous, "Avancer cette page", self.page > 0) {
                self.move_current_page(self.page.saturating_sub(1));
            }
            if action(ui, Icon::Next, "Reculer cette page", self.page + 1 < count) {
                self.move_current_page((self.page + 1).min(count.saturating_sub(1)));
            }
        });
        ui.separator();
        ui.label(egui::RichText::new("Position").size(11.));
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.x = cell;
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(40, 62, 99);
                ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1., Color32::WHITE);
                let field = ui.add(
                    egui::DragValue::new(&mut self.page_destination)
                        .range(1..=count.max(1))
                        .min_decimals(0)
                        .max_decimals(0),
                );
                field.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        count > 0,
                        "Nouvelle position de la page",
                    )
                });
                field.on_hover_text("Nouvelle position de la page");
            });
            if action(ui, Icon::Apply, "Appliquer cette position", count > 0) {
                self.move_current_page(self.page_destination.saturating_sub(1));
            }
        });
    }
    pub(super) fn pick_append_pdfs(&mut self) {
        if self.workspace != Workspace::Pages || !self.finish_text() || !self.permit() {
            return;
        }
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Ajouter un ou plusieurs PDF à la fin")
            .add_filter("Documents PDF", &["pdf"])
            .pick_files()
        {
            self.append_pdf_paths(&paths);
        }
    }
    pub(super) fn append_pdf_paths(&mut self, paths: &[PathBuf]) {
        if self.workspace != Workspace::Pages || !self.finish_text() || !self.permit() {
            return;
        }
        let loaded: Result<Vec<_>> = paths
            .iter()
            .map(|path| {
                pdfinator::files::read(path).map(|bytes| {
                    (
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        bytes,
                    )
                })
            })
            .collect();
        let result = loaded.and_then(|loaded| {
            let sources: Vec<_> = loaded
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.clone()))
                .collect();
            self.editor
                .as_mut()
                .ok_or("Aucun document ouvert.")?
                .append_pdfs(&sources)
        });
        match result {
            Ok(added) if added > 0 => {
                self.page = self.editor.as_ref().unwrap().info.pages.len() - added;
                self.page_destination = self.page + 1;
                self.reset_page_views(true);
                self.jump = Some(self.page);
                self.message = None;
            }
            Ok(_) => {}
            Err(error) => self.message = Some(error),
        }
    }
    pub(super) fn pages_canvas(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let pages = self
            .editor
            .as_ref()
            .map(|e| e.info.pages.clone())
            .unwrap_or_default();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(230, 235, 243))
                    .inner_margin(26.),
            )
            .show(root, |ui| {
                self.canvas = ui.max_rect();
                ui.spacing_mut().item_spacing = Vec2::splat(20.);
                let columns = ((ui.available_width() + 20.) / 206.).floor().max(1.) as usize;
                let card_width = ((ui.available_width() - 20. * (columns - 1) as f32)
                    / columns as f32)
                    .min(250.);
                let card_height = card_width * 1.23 + 40.;
                let mut cards = Vec::new();
                let mut read = None;
                let mut scroll =
                    egui::ScrollArea::vertical().id_salt(("pages-overview", self.active_document));
                if let Some(page) = self.jump.take() {
                    self.page = page.min(pages.len().saturating_sub(1));
                    self.page_destination = self.page + 1;
                    scroll = scroll
                        .vertical_scroll_offset((self.page / columns) as f32 * (card_height + 20.));
                }
                let overview = scroll.show_rows(
                    ui,
                    card_height,
                    pages.len().div_ceil(columns),
                    |ui, rows| {
                        for row in rows {
                            ui.horizontal(|ui| {
                                for (page, p) in pages
                                    .iter()
                                    .enumerate()
                                    .take((row + 1) * columns)
                                    .skip(row * columns)
                                {
                                    let (card, response) = ui.allocate_exact_size(
                                        Vec2::new(card_width, card_height),
                                        egui::Sense::click_and_drag(),
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            true,
                                            format!("Page {} · double-cliquer pour lire", page + 1),
                                        )
                                    });
                                    let mut painter = ui.painter().clone();
                                    if self.page_drag == Some(page) {
                                        painter.multiply_opacity(0.35);
                                    }
                                    self.paint_page_card(
                                        &painter,
                                        card,
                                        p,
                                        page,
                                        page == self.page,
                                        &(page + 1).to_string(),
                                    );
                                    self.thumbnail_visible.push(page);
                                    if response.clicked() || response.drag_started() {
                                        self.page = page;
                                        self.page_destination = page + 1;
                                    }
                                    self.page_context_menu(&response, page);
                                    if response.double_clicked() {
                                        read = Some(page);
                                    }
                                    if response.drag_started() {
                                        self.begin_page_drag(page);
                                    }
                                    if card.intersects(ui.clip_rect()) {
                                        cards.push((page, card));
                                    }
                                }
                            });
                        }
                    },
                );
                let target = self
                    .page_drag
                    .zip(ctx.pointer_interact_pos())
                    .and_then(|(source, pos)| PageDropTarget::closest(source, pos, &cards, false));
                self.page_drag_feedback(ui, &overview, target, &pages, self.canvas);
                if let Some(page) = read {
                    self.open_page(page);
                }
            });
        self.raster.queue(&[], &self.thumbnail_visible, 1., &ctx);
    }
    pub(super) fn page_drag_feedback(
        &mut self,
        ui: &egui::Ui,
        overview: &egui::scroll_area::ScrollAreaOutput<()>,
        target: Option<PageDropTarget>,
        pages: &[PageInfo],
        preview_bounds: Rect,
    ) -> bool {
        let Some(source) = self.page_drag else {
            return false;
        };
        let ctx = ui.ctx();
        let released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let pos = ctx.pointer_interact_pos();
        let inside = pos.is_some_and(|pos| overview.inner_rect.contains(pos));
        let target = target.filter(|_| inside);
        if let Some(target) = target.filter(|t| t.destination != source) {
            let painter = ui.painter().with_clip_rect(overview.inner_rect);
            painter.line_segment(target.marker, Stroke::new(4., icons::BLUE));
            for p in target.marker {
                painter.circle_filled(p, 4., icons::BLUE);
            }
        }
        if released {
            self.page_drag = None;
            if let Some(target) = target {
                let before = self.editor.as_ref().map(Editor::revision);
                self.page = source;
                self.move_current_page(target.destination);
                return before != self.editor.as_ref().map(Editor::revision);
            }
        } else if let Some(pos) = pos {
            ctx.set_cursor_icon(if inside {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::NoDrop
            });
            let size = Vec2::new(146., 190.);
            let origin = (pos + Vec2::new(18., 18.))
                .min(preview_bounds.max - size - Vec2::splat(4.))
                .max(preview_bounds.min + Vec2::splat(4.));
            let ghost = Rect::from_min_size(origin, size);
            let painter = ctx
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("page-drag-preview"),
                ))
                .with_clip_rect(preview_bounds);
            painter.rect_filled(
                ghost.translate(Vec2::new(3., 6.)).expand(3.),
                16.,
                Color32::from_black_alpha(26),
            );
            let label = target.map_or_else(
                || format!("Page {}", source + 1),
                |t| format!("{} → {}", source + 1, t.destination + 1),
            );
            if let Some(p) = pages.get(source) {
                self.paint_page_card(&painter, ghost, p, source, true, &label);
            }
            if inside {
                let edge = 48.;
                let velocity = if pos.y < overview.inner_rect.top() + edge {
                    -((overview.inner_rect.top() + edge - pos.y) / edge).clamp(0., 1.)
                } else {
                    ((pos.y - overview.inner_rect.bottom() + edge) / edge).clamp(0., 1.)
                };
                let max = (overview.content_size.y - overview.inner_rect.height()).max(0.);
                let mut state = overview.state;
                state.offset.y = (state.offset.y
                    + velocity * 620. * ctx.input(|i| i.stable_dt.min(0.05)))
                .clamp(0., max);
                state.store(ctx, overview.id);
                if velocity != 0. {
                    ctx.request_repaint();
                }
            }
        }
        false
    }
    fn paint_page_card(
        &self,
        painter: &egui::Painter,
        card: Rect,
        page: &PageInfo,
        index: usize,
        selected: bool,
        label: &str,
    ) {
        painter.rect_filled(
            card.translate(Vec2::new(0., 3.)),
            14.,
            Color32::from_black_alpha(9),
        );
        painter.rect_filled(
            card,
            14.,
            if selected {
                Color32::from_rgb(219, 232, 255)
            } else {
                Color32::from_rgb(247, 249, 253)
            },
        );
        painter.rect_stroke(
            card,
            14.,
            Stroke::new(
                if selected { 2. } else { 1. },
                if selected {
                    icons::BLUE
                } else {
                    Color32::from_rgb(214, 222, 235)
                },
            ),
            StrokeKind::Inside,
        );
        let area = card.shrink2(Vec2::new(18., 20.));
        let scale = (area.width() / page.width).min((area.height() - 20.) / page.height);
        let sheet = Rect::from_center_size(
            Pos2::new(area.center().x, area.top() + (area.height() - 20.) / 2.),
            Vec2::new(page.width * scale, page.height * scale),
        );
        painter.rect_filled(
            sheet.translate(Vec2::new(0., 2.)),
            1.,
            Color32::from_black_alpha(22),
        );
        painter.rect_filled(sheet, 1., Color32::WHITE);
        if let Some(t) = self.raster.thumbs.get(&index) {
            painter.image(
                t.id(),
                sheet,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        painter.text(
            Pos2::new(card.center().x, card.bottom() - 16.),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.),
            if selected { icons::BLUE } else { icons::INK },
        );
    }
}
