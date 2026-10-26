use super::*;
impl App {
    fn tools(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let enabled = self.editor.is_some();
        egui::Panel::left("tools")
            .exact_size(92.)
            .resizable(false)
            .frame(egui::Frame::new().fill(icons::RAIL).inner_margin(8.))
            .show(root, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4., 4.);
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(218, 230, 249));
                ui.visuals_mut().widgets.noninteractive.bg_stroke =
                    Stroke::new(1., Color32::from_rgb(43, 60, 85));
                // Two columns keep every command reachable at the minimum window height.
                let cell = ((ui.available_height() - 156.) / 13.).clamp(20., 36.);
                let action = |ui: &mut egui::Ui, icon, label: &str, active, available| {
                    icons::button(ui, icon, label, active, available, cell)
                        .on_hover_text(label)
                        .clicked()
                };
                rail_row(ui, cell, |ui| {
                    if action(ui, Icon::Open, "Ouvrir · Ctrl+O", false, true) {
                        self.pick_open();
                    }
                    if action(
                        ui,
                        Icon::Save,
                        "Enregistrer · Ctrl+S",
                        false,
                        enabled && self.work.saving.is_none(),
                    ) {
                        self.save_keep_focus(&ctx, false);
                    }
                });
                rail_row(ui, cell, |ui| {
                    if self.workspace != Workspace::Pages
                        && action(
                            ui,
                            Icon::Print,
                            "Imprimer · Ctrl+P",
                            self.printing.open,
                            enabled,
                        )
                    {
                        self.request_print();
                    }
                    if self.workspace == Workspace::Pages {
                        ui.add_space(cell + 4.);
                    }
                    if action(
                        ui,
                        Icon::Export,
                        "Enregistrer sous · Ctrl+Maj+S",
                        false,
                        enabled,
                    ) {
                        self.save_keep_focus(&ctx, true);
                    }
                });
                ui.separator();
                let tools: &[(Tool, Icon, &str)] = match self.workspace {
                    Workspace::Read => &[
                        (Tool::Select, Icon::Select, "Sélectionner"),
                        (Tool::Hand, Icon::Hand, "Main · déplacer la vue"),
                        (Tool::Highlight, Icon::Highlight, "Surligner"),
                        (Tool::Comment, Icon::Comment, "Commentaire"),
                    ],
                    Workspace::Edit => &[
                        (Tool::Select, Icon::Select, "Sélectionner"),
                        (Tool::Hand, Icon::Hand, "Main · déplacer la vue"),
                        (Tool::Edit, Icon::Edit, "Modifier le texte"),
                        (Tool::Move, Icon::Move, "Déplacer"),
                        (Tool::Text, Icon::Text, "Insérer du texte"),
                        (Tool::Image, Icon::Image, "Insérer une image"),
                        (Tool::Draw, Icon::Drawing, "Dessin"),
                    ],
                    Workspace::Pages => &[],
                };
                for pair in tools.chunks(2) {
                    rail_row(ui, cell, |ui| {
                        for &(tool, icon, label) in pair {
                            if action(ui, icon, label, self.tool == tool, enabled) {
                                self.set_tool(&ctx, tool);
                            }
                        }
                        if self.workspace == Workspace::Edit
                            && pair.last().is_some_and(|p| p.0 == Tool::Draw)
                            && action(
                                ui,
                                Icon::BlankPage,
                                "Ajouter une page blanche",
                                false,
                                enabled,
                            )
                        {
                            self.insert_blank_page();
                        }
                    });
                }
                if self.workspace == Workspace::Edit {
                    let copy = self.selected.as_ref().is_some_and(|s| {
                        !s.is_image && matches!(s.target, Target::Block(_) | Target::Added(_))
                    }) && self.editor.as_ref().is_some_and(Editor::can_copy);
                    rail_row(ui, cell, |ui| {
                        if action(ui, Icon::Copy, "Copier le bloc · Ctrl+C", false, copy) {
                            self.copy_selected_block(&ctx);
                        }
                        if action(
                            ui,
                            Icon::Paste,
                            "Coller le bloc · Ctrl+V",
                            false,
                            enabled && (self.copied_block.is_some() || self.copied_image.is_some()),
                        ) {
                            self.paste_copied_block();
                        }
                    });
                }
                if !tools.is_empty() {
                    ui.separator();
                }
                let undo = self.editor.as_ref().is_some_and(Editor::can_undo);
                let redo = self.editor.as_ref().is_some_and(Editor::can_redo);
                rail_row(ui, cell, |ui| {
                    if action(ui, Icon::Undo, "Annuler · Ctrl+Z", false, undo) {
                        self.undo_redo(false);
                    }
                    if action(ui, Icon::Redo, "Rétablir · Ctrl+Y", false, redo) {
                        self.undo_redo(true);
                    }
                });
                // Zoom, text search and reading navigation do not affect the page organizer.
                if self.workspace == Workspace::Pages {
                    ui.separator();
                    ui.add_enabled_ui(enabled, |ui| self.page_controls(ui, cell));
                    return;
                }
                rail_row(ui, cell, |ui| {
                    if action(
                        ui,
                        Icon::Search,
                        "Rechercher · Ctrl+F",
                        self.search,
                        enabled,
                    ) {
                        if self.search {
                            self.close_search();
                        } else {
                            self.request_search(&ctx);
                        }
                    }
                    let export = icons::button(
                        ui,
                        Icon::Word,
                        "Exporter en DOCX ou ODT",
                        false,
                        enabled && self.word_export.is_none(),
                        cell,
                    )
                    .on_hover_text("Exporter en DOCX ou ODT");
                    egui::Popup::menu(&export).show(|ui| {
                        for format in [pdfinator::docx::Format::Docx, pdfinator::docx::Format::Odt]
                        {
                            if ui
                                .button(format!("{} (.{})", format.label(), format.extension()))
                                .clicked()
                            {
                                ui.close();
                                self.export_word(&ctx, format);
                            }
                        }
                    });
                });
                ui.separator();
                let count = self.editor.as_ref().map_or(0, |e| e.info.pages.len());
                ui.scope(|ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                    ui.visuals_mut().extreme_bg_color = Color32::from_rgb(24, 38, 61);
                    let widgets = &mut ui.visuals_mut().widgets;
                    for w in [
                        &mut widgets.inactive,
                        &mut widgets.hovered,
                        &mut widgets.active,
                    ] {
                        w.fg_stroke = Stroke::new(1., Color32::WHITE);
                        w.bg_fill = Color32::from_rgb(40, 62, 99);
                        w.weak_bg_fill = w.bg_fill;
                    }
                    ui.horizontal(|ui| {
                        let label = format!("/ {count}");
                        let label_width = ui.fonts_mut(|f| {
                            f.layout_no_wrap(
                                label.clone(),
                                egui::FontId::proportional(11.),
                                Color32::WHITE,
                            )
                            .size()
                            .x
                        });
                        ui.add_space(
                            ((ui.available_width()
                                - 34.
                                - label_width
                                - ui.spacing().item_spacing.x)
                                / 2.)
                                .max(0.),
                        );
                        let mut number = if enabled { self.page + 1 } else { 0 };
                        if ui
                            .add_enabled_ui(enabled, |ui| {
                                ui.add_sized(
                                    [34., 24.],
                                    egui::DragValue::new(&mut number).range(if enabled {
                                        1..=count.max(1)
                                    } else {
                                        0..=0
                                    }),
                                )
                            })
                            .inner
                            .on_hover_text("Aller à la page")
                            .changed()
                        {
                            self.jump = Some(number.saturating_sub(1));
                        }
                        ui.label(egui::RichText::new(label).size(11.));
                    });
                });
                rail_row(ui, cell, |ui| {
                    if action(
                        ui,
                        Icon::Previous,
                        "Page précédente",
                        false,
                        enabled && self.page > 0,
                    ) {
                        self.jump = Some(self.page.saturating_sub(1));
                    }
                    if action(
                        ui,
                        Icon::Next,
                        "Page suivante",
                        false,
                        enabled && self.page + 1 < count,
                    ) {
                        self.jump = Some(self.page + 1);
                    }
                });
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.0} %", self.zoom * 100.))
                            .size(11.)
                            .color(Color32::from_rgb(224, 235, 255)),
                    );
                });
                rail_row(ui, cell, |ui| {
                    if action(ui, Icon::Minus, "Réduire le zoom", false, enabled) {
                        self.zoom = (self.zoom / 1.2).max(0.2);
                        self.fit = Fit::Manual;
                    }
                    if action(ui, Icon::Plus, "Augmenter le zoom", false, enabled) {
                        self.zoom = (self.zoom * 1.2).min(4.);
                        self.fit = Fit::Manual;
                    }
                });
                rail_row(ui, cell, |ui| {
                    if action(
                        ui,
                        Icon::Width,
                        "Adapter à la largeur",
                        self.fit == Fit::Width,
                        enabled,
                    ) {
                        self.work.fit_compensation = 0.;
                        self.fit = Fit::Width;
                    }
                    if action(
                        ui,
                        Icon::Page,
                        "Page entière",
                        self.fit == Fit::Page,
                        enabled,
                    ) {
                        self.work.fit_compensation = 0.;
                        self.fit = Fit::Page;
                    }
                });
            });
    }
    fn thumbnails(&mut self, root: &mut egui::Ui) {
        self.thumbnail_visible.clear();
        if !self.thumbnails || self.editor.is_none() || self.workspace == Workspace::Pages {
            return;
        }
        let pages = self
            .editor
            .as_ref()
            .map(|e| e.info.pages.clone())
            .unwrap_or_default();
        egui::Panel::left("thumbnails")
            .exact_size(105.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(246, 248, 252))
                    .inner_margin(9.),
            )
            .show(root, |ui| {
                let reorder = matches!(self.workspace, Workspace::Read | Workspace::Edit);
                let mut cards = Vec::new();
                let overview = egui::ScrollArea::vertical()
                    .id_salt(("page-thumbnails", self.active_document))
                    .show(ui, |ui| {
                        ui.add_space(8.);
                        for (page, p) in pages.iter().enumerate() {
                            let size = Vec2::new(78., (78. * p.height / p.width).clamp(35., 150.));
                            let (rect, response) = ui.allocate_exact_size(
                                size + Vec2::new(0., 22.),
                                if reorder {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::click()
                                },
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    format!("Lire la page {}", page + 1),
                                )
                            });
                            if response.drag_started() && reorder {
                                self.begin_page_drag(page);
                            }
                            if reorder && rect.intersects(ui.clip_rect()) {
                                cards.push((page, rect));
                            }
                            if ui.is_rect_visible(rect) {
                                self.thumbnail_visible.push(page);
                                let mut painter = ui.painter().clone();
                                if self.page_drag == Some(page) {
                                    painter.multiply_opacity(0.35);
                                }
                                let sheet = Rect::from_min_size(rect.min, size);
                                painter.rect_filled(sheet, 2., Color32::WHITE);
                                if let Some(texture) = self.raster.thumbs.get(&page) {
                                    painter.image(
                                        texture.id(),
                                        sheet,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                        Color32::WHITE,
                                    );
                                } else if let Some(frame) = self.raster.frames.get(&page) {
                                    painter.image(
                                        frame.texture.id(),
                                        sheet,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                        Color32::WHITE,
                                    );
                                }
                                painter.rect_stroke(
                                    sheet.expand(2.),
                                    3.,
                                    Stroke::new(
                                        if page == self.page { 2. } else { 1. },
                                        if page == self.page {
                                            icons::BLUE
                                        } else {
                                            Color32::from_gray(220)
                                        },
                                    ),
                                    StrokeKind::Outside,
                                );
                                painter.text(
                                    Pos2::new(rect.center().x, rect.bottom() - 8.),
                                    egui::Align2::CENTER_CENTER,
                                    (page + 1).to_string(),
                                    egui::FontId::proportional(10.),
                                    icons::INK,
                                );
                            }
                            self.page_context_menu(&response, page);
                            if response.clicked() {
                                self.jump = Some(page);
                            }
                            ui.add_space(10.);
                        }
                    });
                let target = self
                    .page_drag
                    .zip(ui.ctx().pointer_interact_pos())
                    .and_then(|(source, pos)| {
                        pages::PageDropTarget::closest(source, pos, &cards, true)
                    });
                if reorder
                    && self.page_drag_feedback(
                        ui,
                        &overview,
                        target,
                        &pages,
                        ui.ctx().content_rect(),
                    )
                {
                    self.jump = Some(self.page);
                }
            });
    }
    fn right_panel(&mut self, root: &mut egui::Ui) {
        self.right_rect = Rect::NOTHING;
        if self
            .drawing
            .drag
            .as_ref()
            .is_some_and(|g| !g.committed && !g.panel_visible)
        {
            return;
        }
        // Opening a sidebar during placement would shift the page under the pointer.
        if self
            .drag
            .as_ref()
            .is_some_and(|d| matches!(d.mode, DragMode::Create))
        {
            return;
        }
        let selection_panel = self.tool == Tool::Draw
            || self.image_crop.is_some()
            || !self.drawing.selected.is_empty()
            || self.selected.as_ref().is_some_and(|s| {
                matches!(s.target, Target::Highlight(_) | Target::Comment(_))
                    || (self.workspace == Workspace::Edit
                        && !matches!(self.tool, Tool::Select | Tool::Move | Tool::Hand))
            });
        if self.editor.is_none() || !(selection_panel || self.search || self.tool == Tool::Comment)
        {
            return;
        }
        if self.properties_collapsed {
            let panel = egui::Panel::right("properties-collapsed")
                .exact_size(32.)
                .resizable(false)
                .show(root, |ui| {
                    if ui
                        .button("‹")
                        .on_hover_text("Déplier les réglages")
                        .clicked()
                    {
                        self.properties_collapsed = false;
                    }
                });
            self.right_rect = panel.response.rect;
            return;
        }
        let panel = egui::Panel::right("properties")
            .default_size(246.)
            .min_size(220.)
            .max_size(360.)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(247, 249, 253))
                    .inner_margin(14.),
            )
            .show(root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("properties-scroll", self.active_document))
                    .show(ui, |ui| {
                        if ui.button("Replier ›").clicked() {
                            self.properties_collapsed = true;
                        }
                        if self.image_crop.is_some() {
                            self.image_crop_controls(ui);
                            return;
                        }
                        if self.search {
                            ui.heading("Rechercher");
                            ui.add_space(10.);
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.query)
                                        .id(egui::Id::new("find"))
                                        .hint_text("Mot ou phrase")
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                self.find();
                            }
                            if self.work.searching.is_some() || self.work.search_due.is_some() {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Recherche…");
                                });
                            } else {
                                ui.label(format!("{} résultat(s)", self.hits.len()));
                            }
                            if icons::small(ui, Icon::Next, "Suivant").clicked()
                                && !self.hits.is_empty()
                            {
                                self.hit = (self.hit + 1) % self.hits.len();
                                self.jump = self.hits.get(self.hit).map(|h| h.0);
                            }
                            if icons::small(ui, Icon::Close, "Fermer").clicked() {
                                self.close_search();
                            }
                            return;
                        }
                        if self.tool == Tool::Draw || !self.drawing.selected.is_empty() {
                            self.drawing_panel(ui);
                            return;
                        }
                        if self.tool == Tool::Comment
                            || self
                                .selected
                                .as_ref()
                                .is_some_and(|s| matches!(s.target, Target::Comment(_)))
                        {
                            self.comments_panel(ui);
                            return;
                        }
                        let Some(s) = self
                            .inline
                            .as_ref()
                            .filter(|i| !i.committed)
                            .map(|i| i.selection.clone())
                            .or_else(|| self.selected.clone())
                        else {
                            return;
                        };
                        if s.is_image || matches!(s.target, Target::Highlight(_)) {
                            let title = if s.is_image {
                                "Image"
                            } else if matches!(s.target, Target::Highlight(_)) {
                                "Surlignage"
                            } else {
                                "Texte"
                            };
                            ui.horizontal(|ui| {
                                ui.strong(title);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if icons::small(ui, Icon::Close, "")
                                            .on_hover_text("Fermer les réglages")
                                            .clicked()
                                            && self.finish_text()
                                        {
                                            self.selected = None;
                                        }
                                    },
                                );
                            });
                            ui.add_space(12.);
                        }
                        if s.is_image {
                            ui.checkbox(&mut self.lock_aspect, "Conserver les proportions");
                            if let Some(p) =
                                self.editor.as_ref().and_then(|e| e.info.pages.get(s.page))
                            {
                                let rect = s.view_rect(p);
                                let mut width = rect.width();
                                let mut height = rect.height();
                                ui.label("Dimensions");
                                let wc = ui
                                    .add(
                                        egui::DragValue::new(&mut width)
                                            .range(8.0..=10000.)
                                            .prefix("L : ")
                                            .suffix(" pt"),
                                    )
                                    .changed();
                                let hc = ui
                                    .add(
                                        egui::DragValue::new(&mut height)
                                            .range(8.0..=10000.)
                                            .prefix("H : ")
                                            .suffix(" pt"),
                                    )
                                    .changed();
                                if wc || hc {
                                    if self.lock_aspect {
                                        if wc {
                                            height = width * rect.height() / rect.width().max(1.);
                                        } else {
                                            width = height * rect.width() / rect.height().max(1.);
                                        }
                                    }
                                    self.transform(
                                        s.clone(),
                                        Rect::from_min_size(rect.min, Vec2::new(width, height)),
                                    );
                                }
                            }
                            ui.add_space(12.);
                            if self.workspace == Workspace::Edit && self.tool == Tool::Edit {
                                if icons::small(ui, Icon::Edit, "Modifier le texte de l’image")
                                    .clicked()
                                {
                                    self.start_recognition(ui.ctx(), s.clone());
                                }
                                if icons::small(ui, Icon::Text, "Textes des images de la page")
                                    .clicked()
                                {
                                    self.start_page_recognition(ui.ctx(), s.page);
                                }
                                ui.add_space(6.);
                            }
                            if icons::small(ui, Icon::Replace, "Remplacer").clicked() {
                                self.replace_image();
                            }
                            if icons::small(ui, Icon::Delete, "Supprimer").clicked() {
                                self.delete();
                            }
                        } else if let Target::Highlight(index) = s.target {
                            let mut color = s.color;
                            if self.palette(ui, &mut color) && self.permit() {
                                if let Some(e) = &mut self.editor {
                                    let result =
                                        e.color_highlight(s.page, index, color.map(f64::from));
                                    self.changed(result, s.page);
                                    if let Some(s) = &mut self.selected {
                                        s.color = color;
                                    }
                                }
                            }
                            if icons::small(ui, Icon::Delete, "Supprimer").clicked() {
                                self.delete();
                            }
                        } else {
                            self.text_controls(ui, &s);
                        }
                    });
            });
        self.right_rect = panel.response.rect;
    }
    fn replace_image(&mut self) {
        let Some(s) = self.selected.clone() else {
            return;
        };
        if !self.permit() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        if let Some(e) = &mut self.editor {
            let result = match s.target {
                Target::Added(i) => e.image(s.page, s.rect, &path, Some(i)),
                Target::Image(i) => {
                    let bytes = match pdfinator::files::read(&path) {
                        Ok(b) => b,
                        Err(e) => {
                            self.message = Some(e);
                            return;
                        }
                    };
                    e.change(false, |d| {
                        let (image, _) =
                            pdfcraft_create::image_xobject(d, &path.to_string_lossy(), &bytes)
                                .map_err(|e| e.to_string())?;
                        pdfcraft_edit::change_image(
                            d,
                            s.page,
                            i,
                            &pdfcraft_edit::ImageChange::Replace(image),
                        )
                        .map_err(|e| e.to_string())
                    })
                }
                _ => return,
            };
            self.changed(result, s.page);
        }
    }
    pub(super) fn select_next(&mut self, ctx: &egui::Context) {
        if !self.finish_text() {
            return;
        }
        let list = self
            .editor
            .as_ref()
            .and_then(|e| pdfcraft_edit::editable_objects(&e.doc, self.page).ok())
            .unwrap_or_default();
        if list.is_empty() {
            return;
        }
        let current = self.selected.as_ref().map(|s| s.target);
        let targets: Vec<_> = list
            .iter()
            .map(|o| match o.target.kind {
                ObjectKind::Text => Target::Block(o.target.index),
                ObjectKind::Added => Target::Added(o.target.index),
                ObjectKind::Image => Target::Image(o.target.index),
            })
            .collect();
        let i = current
            .and_then(|t| targets.iter().position(|x| *x == t))
            .map_or(0, |i| (i + 1) % targets.len());
        if let Some(s) = targets
            .get(i)
            .and_then(|t| self.selection_for(self.page, *t))
        {
            self.selected = Some(s.clone());
            if self.tool == Tool::Edit {
                self.begin_text(ctx, s);
            }
        }
    }
}
impl App {
    pub(super) fn frame_ui(&mut self, root: &mut egui::Ui) {
        self.refresh_image_clipboard(root.ctx());
        let ctx = root.ctx().clone();
        self.poll_work(&ctx);
        self.fonts.poll(&ctx);
        let printing_was_busy = self.printing.busy;
        self.printing.poll();
        if printing_was_busy && self.printing.failed {
            self.work.exit_after_jobs = false;
        }
        self.poll_word_export(&ctx);
        if self.work.exit_after_jobs
            && self.word_export.is_none()
            && !self.printing.busy
            && self.work.saving.is_none()
        {
            self.request_exit(&ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_confirmed {
            self.request_exit(&ctx);
            if !self.close_confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if let Some(path) =
            ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()))
        {
            self.open(path);
        }
        // Outside clicks commit the in-place editor, while property controls keep it active.
        if self.inline.as_ref().is_some_and(|i| !i.committed) && self.drag.is_none() {
            let outside = ctx
                .input(|i| {
                    i.pointer
                        .any_pressed()
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                })
                .is_some_and(|p| {
                    self.canvas.contains(p)
                        && !self.right_rect.contains(p)
                        && self
                            .inline
                            .as_ref()
                            .is_some_and(|i| !i.screen.expand(9.).contains(p))
                });
            if outside {
                self.finish_text();
            }
        }
        self.shortcuts(&ctx);
        self.page_drag_input(&ctx);
        for error in self.raster.poll(&ctx) {
            self.message = Some(error);
        }
        if let Some(error) = self
            .marquee
            .poll(&ctx, self.raster.current(self.marquee.page))
        {
            self.message = Some(error);
        }
        if let Some(g) = &mut self.drawing.drag {
            if let Some(b) = &mut g.background {
                if let Some(error) = b.poll(&ctx) {
                    self.message = Some(error);
                }
            }
            if g.committed && self.raster.current(g.page) {
                self.drawing.drag = None;
            }
        }
        if let Some(i) = &mut self.inline {
            if let Some(error) = i.background.poll(&ctx) {
                self.message = Some(error);
            }
            if i.committed && self.raster.current(i.selection.page) {
                self.inline = None;
            }
        }
        if let Some(d) = &mut self.drag {
            if let Some(error) = d.background.poll(&ctx) {
                self.message = Some(error);
            }
        }
        if self
            .drag
            .as_ref()
            .is_some_and(|d| d.committed && self.raster.current(d.selection.page))
        {
            self.drag = None;
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("animations"), self.animations));
        self.tabs(root);
        self.work_status(root);
        self.export_status(root);
        self.tools(root);
        self.thumbnails(root);
        // Fit and centering use the space that remains after the property panel.
        self.right_panel(root);
        let remaining = (root.available_width() - 24.).max(1.);
        let pane = if self.right_rect.is_positive() {
            self.right_rect.width()
        } else {
            0.
        };
        let delta = pane - self.work.panel_width;
        if delta.abs() > 0.1 {
            self.work.fit_compensation += delta;
            if let Some(page) = self
                .editor
                .as_ref()
                .and_then(|e| e.info.pages.get(self.page))
            {
                let sheet = page.width * self.zoom;
                let old_margin = ((remaining + delta - 18. - sheet) / 2.).max(0.);
                let new_margin = ((remaining - 18. - sheet) / 2.).max(0.);
                self.scroll_offset.x =
                    (self.scroll_offset.x + delta / 2. + new_margin - old_margin).max(0.);
            }
        }
        self.work.panel_width = pane;
        self.document_width = (remaining + self.work.fit_compensation).max(1.);
        if self.workspace == Workspace::Pages && self.editor.is_some() {
            self.pages_canvas(root);
        } else {
            self.document_canvas(root);
        }
        if self.printing.open {
            self.print_dialog(&ctx);
        }
        self.recognition_dialog(&ctx);
        self.observe_recovery(&ctx);
        self.recovery_dialog(&ctx);
        if self.printing.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
        if let Some(message) = self.message.clone() {
            let right = self.canvas.right() - 16.;
            let bottom = self.canvas.bottom() - 16.;
            egui::Area::new(egui::Id::new("message"))
                .fixed_pos(Pos2::new(
                    (right - 380.).max(self.canvas.left() + 8.),
                    (bottom - 110.).max(self.canvas.top() + 8.),
                ))
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(1., Color32::from_rgb(218, 226, 238)))
                        .corner_radius(8.)
                        .inner_margin(12.)
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 4],
                            blur: 18,
                            spread: 0,
                            color: Color32::from_black_alpha(30),
                        })
                        .show(ui, |ui| {
                            ui.set_width(340.);
                            ui.horizontal(|ui| {
                                ui.strong("PDFinator");
                                if icons::small(ui, Icon::Close, "")
                                    .on_hover_text("Fermer le message")
                                    .clicked()
                                {
                                    self.message = None;
                                }
                            });
                            ui.label(message);
                        });
                });
        }
        let name = self
            .editor
            .as_ref()
            .and_then(|e| e.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("PDFinator — {name}")));
    }
}
impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_ui(root);
    }
}

fn rail_row<R>(
    ui: &mut egui::Ui,
    cell: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal(|ui| {
        ui.add_space(
            ((ui.available_width() - cell * 2. - ui.spacing().item_spacing.x) / 2.).max(0.),
        );
        content(ui)
    })
}
