use super::*;
impl App {
    pub(super) fn document_canvas(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let mut visible = Vec::new();
        let mut surfaces = Vec::new();
        let mut hovered_text = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(227, 234, 244))
                    .inner_margin(12.),
            )
            .show(root, |ui| {
                self.canvas = ui.max_rect();
                let Some(e) = &self.editor else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.28).max(30.));
                        let r = ui
                            .allocate_exact_size(Vec2::splat(62.), egui::Sense::hover())
                            .0;
                        icons::draw(ui.painter(), Icon::Open, r, icons::BLUE);
                        ui.add_space(16.);
                        ui.label(
                            egui::RichText::new("Ouvrir un document PDF")
                                .size(23.)
                                .strong(),
                        );
                        ui.add_space(9.);
                        ui.label("Glissez votre PDF dans cette fenêtre ou choisissez un fichier.");
                        ui.add_space(18.);
                        if icons::small(ui, Icon::Open, "Choisir un fichier…").clicked() {
                            self.pick_open();
                        }
                    });
                    return;
                };
                let pages = e.info.pages.clone();
                let width = ui.available_width();
                if let Some(p) = pages.get(self.page) {
                    match self.fit {
                        Fit::Width => {
                            self.zoom = ((self.document_width - 28.) / p.width).clamp(0.2, 4.)
                        }
                        Fit::Page => {
                            self.zoom = ((self.document_width - 28.) / p.width)
                                .min((ui.available_height() - 24.) / p.height)
                                .clamp(0.2, 4.)
                        }
                        Fit::Manual => {}
                    }
                }
                // Read pinch / Ctrl+wheel before laying out the scroll area. Zoom the
                // document, not the interface, and keep the PDF point under the pointer.
                if ui.rect_contains_pointer(ui.max_rect()) {
                    let delta = ctx.input(|i| i.zoom_delta());
                    if delta.is_finite() && delta > 0. && (delta - 1.).abs() > 0.0001 {
                        let old = self.zoom;
                        self.zoom = (old * delta).clamp(0.2, 4.);
                        self.fit = Fit::Manual;
                        let pointer = ctx.pointer_hover_pos().unwrap_or(ui.max_rect().center())
                            - ui.max_rect().min;
                        let sizes: Vec<_> =
                            pages.iter().map(|p| Vec2::new(p.width, p.height)).collect();
                        self.scroll_offset =
                            zoom_offset(self.scroll_offset, pointer, width, old, self.zoom, &sizes);
                        ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
                    }
                }
                let scroll = egui::ScrollArea::both()
                    .id_salt("document-scroll")
                    .scroll_source(if self.tool == Tool::Hand {
                        egui::scroll_area::ScrollSource::ALL
                    } else {
                        egui::scroll_area::ScrollSource::default()
                    })
                    .on_hover_cursor(egui::CursorIcon::Default)
                    .on_drag_cursor(egui::CursorIcon::Default)
                    .scroll_offset(self.scroll_offset)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Match the fixed 20pt page gap used by pointer-anchored zoom.
                        ui.spacing_mut().item_spacing.y = 0.;
                        ui.set_min_width(width - 1.);
                        for (page, p) in pages.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_space(((width - 18. - p.width * self.zoom) / 2.).max(0.));
                                if self.page_ui(ui, page, p, &mut surfaces) {
                                    visible.push(page);
                                    if let Some(pos) = ctx.pointer_hover_pos().filter(|pos| {
                                        surfaces.last().is_some_and(|r| r.contains(*pos))
                                    }) {
                                        let q = (pos - surfaces.last().unwrap().min) / self.zoom;
                                        hovered_text |= self.text.get(&page).is_some_and(|t| {
                                            t.glyphs.iter().any(|g| {
                                                !g.text.trim().is_empty()
                                                    && q.x >= g.rect[0]
                                                    && q.x <= g.rect[2]
                                                    && q.y >= g.rect[1]
                                                    && q.y <= g.rect[3]
                                            })
                                        });
                                    }
                                }
                            });
                            ui.add_space(20.);
                        }
                    });
                self.scroll_offset = scroll.state.offset;
                if self.workspace == Workspace::Read
                    && self.tool == Tool::Select
                    && !self.printing.open
                    && ctx.pointer_hover_pos().is_some_and(|p| {
                        scroll.inner_rect.contains(p) && ctx.layer_id_at(p) == Some(ui.layer_id())
                    })
                {
                    ctx.set_cursor_icon(if hovered_text {
                        egui::CursorIcon::Text
                    } else {
                        egui::CursorIcon::Default
                    });
                }
                if self.tool == Tool::Highlight && !self.printing.open && self.recognition.is_none()
                {
                    if let Some(pos) = ctx
                        .pointer_hover_pos()
                        .filter(|p| scroll.inner_rect.contains(*p))
                    {
                        ctx.set_cursor_icon(if surfaces.iter().any(|r| r.contains(pos)) {
                            egui::CursorIcon::Text
                        } else {
                            egui::CursorIcon::Default
                        });
                    }
                }
                if matches!(self.tool, Tool::Hand | Tool::Move)
                    && !self.printing.open
                    && self.recognition.is_none()
                    && self.image_crop.is_none()
                    && ctx.pointer_hover_pos().is_some_and(|p| {
                        scroll.inner_rect.contains(p) && ctx.layer_id_at(p) == Some(ui.layer_id())
                    })
                {
                    let on_page = ctx
                        .pointer_hover_pos()
                        .is_some_and(|p| surfaces.iter().any(|r| r.contains(p)));
                    ctx.set_cursor_icon(if !on_page {
                        egui::CursorIcon::Default
                    } else if ctx.input(|i| i.pointer.primary_down()) {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                }
                let gray_click = ctx
                    .input(|i| {
                        i.pointer
                            .any_pressed()
                            .then(|| i.pointer.interact_pos())
                            .flatten()
                    })
                    .is_some_and(|pos| {
                        scroll.inner_rect.contains(pos)
                            && !surfaces.iter().any(|r: &Rect| r.contains(pos))
                    });
                if gray_click {
                    self.deselect();
                }
            });
        self.raster.queue(
            &visible,
            &self.thumbnail_visible,
            (self.zoom * ctx.pixels_per_point()).clamp(0.25, 4.),
            &ctx,
        );
    }
    fn inventory(&mut self, page: usize) {
        let Some(e) = &self.editor else { return };
        if let std::collections::hash_map::Entry::Vacant(entry) = self.objects.entry(page) {
            match pdfinator::document::guarded(|| {
                pdfcraft_edit::editable_objects(&e.doc, page).map_err(|e| e.to_string())
            }) {
                Ok(mut list) => {
                    if let Ok(blocks) = e.blocks(page) {
                        for object in &mut list {
                            if object.target.kind == ObjectKind::Text {
                                if let Some(block) = blocks.get(object.target.index) {
                                    object.rect = block.rect;
                                }
                            }
                        }
                    }
                    entry.insert(list);
                }
                Err(e) => self.message = Some(format!("Sélection limitée sur cette page : {e}")),
            }
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = self.text.entry(page) {
            if let Ok(t) = e.text(page) {
                entry.insert(t);
            }
        }
    }
    pub(super) fn hit(&self, page: usize, p: &PageInfo, point: Pos2) -> Option<Selection> {
        let user = p.view_to_user(point.x, point.y);
        if matches!(self.tool, Tool::Highlight | Tool::Comment | Tool::Select) {
            if let Some(e) = &self.editor {
                if let Some(a) = e.info.annotations.iter().rev().find(|a| {
                    a.page == page
                        && user[0] >= a.rect[0]
                        && user[0] <= a.rect[2]
                        && user[1] >= a.rect[1]
                        && user[1] <= a.rect[3]
                }) {
                    if a.subtype == "Text" {
                        return self.selection_for(page, Target::Comment(a.index));
                    }
                    if self.tool == Tool::Highlight {
                        return self.selection_for(page, Target::Highlight(a.index));
                    }
                }
            }
        }
        let objects = self.objects.get(&page)?;
        let object = objects
            .iter()
            .filter(|o| {
                user[0] as f64 >= o.rect[0]
                    && user[0] as f64 <= o.rect[2]
                    && user[1] as f64 >= o.rect[1]
                    && user[1] as f64 <= o.rect[3]
            })
            .min_by_key(|o| match o.target.kind {
                ObjectKind::Text => 0,
                ObjectKind::Added if o.content_type == "text" => 0,
                ObjectKind::Added => 1,
                ObjectKind::Image => 2,
            })?;
        self.selection_for(
            page,
            match object.target.kind {
                ObjectKind::Text => Target::Block(object.target.index),
                ObjectKind::Added => Target::Added(object.target.index),
                ObjectKind::Image => Target::Image(object.target.index),
            },
        )
    }
    fn page_ui(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        p: &PageInfo,
        surfaces: &mut Vec<Rect>,
    ) -> bool {
        let size = Vec2::new(p.width, p.height);
        let (rect, response) = ui.allocate_exact_size(
            size * self.zoom,
            if self.tool == Tool::Hand {
                egui::Sense::click()
            } else {
                egui::Sense::click_and_drag()
            },
        );
        if self.jump == Some(page) {
            ui.scroll_to_rect(
                Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.)),
                Some(egui::Align::TOP),
            );
            self.jump = None;
        }
        if !ui.is_rect_visible(rect) {
            return false;
        }
        surfaces.push(rect);
        if rect.top() <= ui.clip_rect().top() + rect.height() * 0.5
            && rect.bottom() > ui.clip_rect().top()
        {
            self.page = page;
        }
        self.inventory(page);
        ui.painter().add(
            egui::epaint::Shadow {
                offset: [0, 3],
                blur: 13,
                spread: 0,
                color: Color32::from_black_alpha(32),
            }
            .as_shape(rect, 1.),
        );
        ui.painter().rect_filled(rect, 1., Color32::WHITE);
        let mut texture = self.raster.frames.get(&page).map(|f| f.texture.clone());
        if let Some(t) = self.marquee.page_texture(page) {
            texture = Some(t.clone());
        }
        if let Some(g) = self.drawing.drag.as_ref().filter(|g| g.page == page) {
            if let Some(t) = g.background.as_ref().and_then(|b| b.texture.as_ref()) {
                texture = Some(t.clone());
            }
        }
        if let Some(i) = &self.inline {
            if i.selection.page == page {
                if let Some(t) = &i.background.texture {
                    texture = Some(t.clone());
                }
            }
        }
        if let Some(d) = &self.drag {
            if d.selection.page == page {
                if let Some(t) = &d.background.texture {
                    texture = Some(t.clone());
                }
            }
        }
        if let Some(texture) = texture {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        let screen = |r: Rect, zoom: f32| {
            Rect::from_min_max(
                rect.min + r.min.to_vec2() * zoom,
                rect.min + r.max.to_vec2() * zoom,
            )
        };
        let glyph_rect =
            |r: [f32; 4]| Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3]));
        if let Some(t) = self.text.get(&page) {
            for (_, range) in self.hits.iter().filter(|(n, _)| self.search && *n == page) {
                for r in t.line_rects(range.clone()) {
                    ui.painter().rect_filled(
                        screen(glyph_rect(r), self.zoom),
                        0.,
                        Color32::from_rgba_unmultiplied(255, 210, 0, 95),
                    );
                }
            }
            if let Some((n, a, b)) = self.text_selection {
                if n == page {
                    for r in t.line_rects(a.min(b)..a.max(b) + 1) {
                        ui.painter().rect_filled(
                            screen(glyph_rect(r), self.zoom),
                            0.,
                            Color32::from_rgba_unmultiplied(61, 113, 255, 72),
                        );
                    }
                }
            }
        }
        self.image_page_menu(ui, page, p, rect, &response);
        if self.image_crop_canvas(ui, page, rect) {
            return true;
        }
        if self.workspace == Workspace::Edit && self.tool == Tool::Select {
            self.mixed_canvas(ui, page, p, rect, &response);
            return true;
        }
        if self.workspace == Workspace::Edit
            && self.tool == Tool::Edit
            && self.mixed_edit_capture(ui, page, p, rect, &response)
        {
            if !self.finish_text() {
                return true;
            }
            self.mixed_canvas(ui, page, p, rect, &response);
            return true;
        }
        if self.tool == Tool::Draw
            || (self.workspace == Workspace::Edit
                && matches!(self.tool, Tool::Edit | Tool::Select | Tool::Move)
                && self.drawing_capture(ui, page, p, rect, &response))
        {
            if self.tool != Tool::Draw {
                self.drawing.mode = None;
            }
            self.drawing_canvas(ui, page, p, rect, &response);
            return true;
        }
        let edit = matches!(
            self.tool,
            Tool::Edit | Tool::Text | Tool::Image | Tool::Move
        );
        if edit {
            if let Some(objects) = self.objects.get(&page) {
                for object in objects {
                    let target = match object.target.kind {
                        ObjectKind::Text => Target::Block(object.target.index),
                        ObjectKind::Added => Target::Added(object.target.index),
                        ObjectKind::Image => Target::Image(object.target.index),
                    };
                    if self
                        .drag
                        .as_ref()
                        .is_some_and(|d| d.selection.page == page && d.selection.target == target)
                    {
                        continue;
                    }
                    let mut s = Selection::text(
                        page,
                        target,
                        object.rect,
                        String::new(),
                        12.,
                        [0.; 3],
                        false,
                    );
                    // Inventories use user space even for added objects.
                    s.target = Target::Block(usize::MAX);
                    let bounds = screen(s.view_rect(p), self.zoom);
                    ui.painter().rect_stroke(
                        bounds.expand(1.),
                        0.,
                        Stroke::new(0.8, Color32::from_rgba_unmultiplied(48, 94, 245, 75)),
                        StrokeKind::Outside,
                    );
                }
            }
        }
        let point = response.interact_pointer_pos().map(|pos| {
            Pos2::new(
                (pos.x - rect.left()) / self.zoom,
                (pos.y - rect.top()) / self.zoom,
            )
        });
        if response.drag_started_by(egui::PointerButton::Primary) && self.tool == Tool::Text {
            if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                let q = (start - rect.min) / self.zoom;
                if self.hit(page, p, Pos2::new(q.x, q.y)).is_none() {
                    let selection = Selection::text(
                        page,
                        Target::New,
                        [
                            q.x as f64,
                            (p.height - q.y) as f64,
                            q.x as f64,
                            (p.height - q.y) as f64,
                        ],
                        String::new(),
                        14.,
                        [0.; 3],
                        false,
                    );
                    self.begin_drag(ui.ctx(), selection, DragMode::Create, start, rect, size);
                }
            }
        }
        if response.drag_started() && self.tool == Tool::Move {
            if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                let q = (start - rect.min) / self.zoom;
                if let Some(s) = self.hit(page, p, Pos2::new(q.x, q.y)) {
                    self.selected = Some(s.clone());
                    self.begin_drag(ui.ctx(), s, DragMode::Move, start, rect, size);
                }
            }
        }
        if response.drag_started() && matches!(self.tool, Tool::Select | Tool::Highlight) {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                let q = (pos - rect.min) / self.zoom;
                if let Some(index) = self.text.get(&page).and_then(|t| t.nearest(q.x, q.y)) {
                    self.text_selection = Some((page, index, index));
                }
            }
        }
        if response.dragged() && matches!(self.tool, Tool::Select | Tool::Highlight) {
            if let Some(point) = point {
                if let Some(index) = self
                    .text
                    .get(&page)
                    .and_then(|t| t.nearest(point.x, point.y))
                {
                    if let Some((n, a, _)) = self.text_selection {
                        self.text_selection = Some((n, a, index));
                    }
                }
            }
        }
        if response.drag_stopped() && self.tool == Tool::Highlight && self.permit() {
            if let Some((n, a, b)) = self.text_selection {
                if let (Some(e), Some(t)) = (&mut self.editor, self.text.get(&n)) {
                    let result = e.toggle_highlight(n, t, a.min(b)..a.max(b) + 1);
                    self.changed(result, n);
                }
            }
        }
        if response.clicked() && self.drag.is_none() && self.tool != Tool::Hand {
            if let Some(point) = point {
                if self.inline.as_ref().is_some_and(|i| {
                    !i.committed
                        && i.selection.page == page
                        && i.screen
                            .contains(response.interact_pointer_pos().unwrap_or(Pos2::ZERO))
                }) {
                    return true;
                }
                if response.double_clicked()
                    && self.workspace == Workspace::Edit
                    && self.tool == Tool::Edit
                {
                    let image = self
                        .selected
                        .clone()
                        .filter(|s| s.is_image && s.page == page)
                        .or_else(|| self.hit(page, p, point).filter(|s| s.is_image));
                    if let Some(s) = image {
                        self.start_automatic_recognition(ui.ctx(), s);
                        return true;
                    }
                }
                self.marquee.clear();
                if self.tool == Tool::Image {
                    self.place_image(page, p, point);
                } else if self.tool == Tool::Comment {
                    if let Some(s) = self
                        .hit(page, p, point)
                        .filter(|s| matches!(s.target, Target::Comment(_)))
                    {
                        if self.finish_text() {
                            self.selected = Some(s);
                        }
                    } else {
                        self.place_comment(page, p, point);
                    }
                } else if let Some(s) = self.selected.clone().filter(|s| {
                    matches!(s.target, Target::New)
                        && s.page == page
                        && s.view_rect(p).contains(point)
                }) {
                    self.begin_text(ui.ctx(), s);
                } else if let Some(s) = self.hit(page, p, point) {
                    if !self.finish_text() {
                        return true;
                    }
                    self.selected = Some(s.clone());
                    if matches!(self.tool, Tool::Edit | Tool::Text) && !s.is_image {
                        self.begin_text(ui.ctx(), s);
                    }
                } else if self.tool == Tool::Text {
                    let x = point.x.max(0.) as f64;
                    let y = (p.height - point.y) as f64;
                    let s = Selection::text(
                        page,
                        Target::New,
                        [x, y - 24., (x + 250.).min(p.width as f64), y],
                        String::new(),
                        14.,
                        [0.; 3],
                        false,
                    );
                    self.begin_text(ui.ctx(), s);
                } else {
                    self.deselect();
                }
            }
        }
        // Draw the actual source-free inline text, with no box fill, margin or new bubble.
        if let Some(i) = &mut self.inline {
            if i.selection.page == page {
                if i.draw(ui, rect, self.zoom).is_some() {
                    self.selected = Some(i.selection.clone());
                }
            }
        }
        if let Some(d) = &self.drag {
            if d.selection.page == page {
                let galley = d.text_preview.as_ref().and_then(|preview| {
                    preview.layout(
                        ui,
                        &d.selection.draft,
                        self.zoom,
                        d.current.width().max(4.) * self.zoom,
                    )
                });
                if let Some(galley) = galley {
                    ui.painter()
                        .galley(screen(d.current, self.zoom).min, galley, Color32::BLACK);
                } else if let Some(texture) = &d.texture {
                    let uv = Rect::from_min_max(
                        Pos2::new(
                            d.source.left() / d.page_size.x,
                            d.source.top() / d.page_size.y,
                        ),
                        Pos2::new(
                            d.source.right() / d.page_size.x,
                            d.source.bottom() / d.page_size.y,
                        ),
                    );
                    ui.painter().image(
                        texture.id(),
                        screen(d.current, self.zoom),
                        uv,
                        Color32::WHITE,
                    );
                }
            }
        }
        if let Some(s) = self.selected.clone().filter(|s| s.page == page) {
            if !matches!(s.target, Target::Highlight(_) | Target::Comment(_)) && edit {
                let local = self
                    .drag
                    .as_ref()
                    .filter(|d| d.selection.page == page)
                    .map_or_else(|| s.view_rect(p), |d| d.current);
                let outline = self
                    .inline
                    .as_ref()
                    .filter(|i| {
                        i.selection.page == page
                            && i.selection.target == s.target
                            && !i.screen.is_negative()
                    })
                    .map_or_else(|| screen(local, self.zoom), |i| i.screen);
                self.frame_controls(ui, s, outline, rect, size);
            }
        }
        self.update_drag(ui, rect, page);
        true
    }
    fn frame_controls(
        &mut self,
        ui: &mut egui::Ui,
        s: Selection,
        outline: Rect,
        page_rect: Rect,
        page_size: Vec2,
    ) {
        ui.painter().rect_stroke(
            outline.expand(1.),
            0.,
            Stroke::new(1.3, icons::BLUE),
            StrokeKind::Outside,
        );
        let corners = [
            outline.left_top(),
            outline.right_top(),
            outline.right_bottom(),
            outline.left_bottom(),
        ];
        for (index, corner) in corners.into_iter().enumerate() {
            ui.painter().circle_filled(corner, 3.5, Color32::WHITE);
            ui.painter()
                .circle_stroke(corner, 3.5, Stroke::new(1.5, icons::BLUE));
            let response = ui.interact(
                Rect::from_center_size(corner, Vec2::splat(15.)),
                egui::Id::new(("resize", s.page, format!("{:?}", s.target), index)),
                egui::Sense::click_and_drag(),
            );
            self.block_frame_menu(ui, &response, &s);
            if response.hovered() {
                ui.ctx().set_cursor_icon(if index % 2 == 0 {
                    egui::CursorIcon::ResizeNwSe
                } else {
                    egui::CursorIcon::ResizeNeSw
                });
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                    self.begin_drag(
                        ui.ctx(),
                        s.clone(),
                        DragMode::Corner(index),
                        start,
                        page_rect,
                        page_size,
                    );
                }
            }
        }
        let edges = [
            Rect::from_min_max(
                Pos2::new(outline.left() + 10., outline.top() - 6.),
                Pos2::new(outline.right() - 10., outline.top() + 4.),
            ),
            Rect::from_min_max(
                Pos2::new(outline.left() + 10., outline.bottom() - 4.),
                Pos2::new(outline.right() - 10., outline.bottom() + 6.),
            ),
            Rect::from_min_max(
                Pos2::new(outline.left() - 6., outline.top() + 10.),
                Pos2::new(outline.left() + 4., outline.bottom() - 10.),
            ),
            Rect::from_min_max(
                Pos2::new(outline.right() - 4., outline.top() + 10.),
                Pos2::new(outline.right() + 6., outline.bottom() - 10.),
            ),
        ];
        for (index, edge) in edges.into_iter().enumerate() {
            if !edge.is_positive() {
                continue;
            }
            let response = ui.interact(
                edge,
                egui::Id::new(("move-edge", s.page, index)),
                egui::Sense::click_and_drag(),
            );
            self.block_frame_menu(ui, &response, &s);
            if response.hovered() {
                ui.ctx().set_cursor_icon(moving_cursor(ui.ctx()));
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                    self.begin_drag(
                        ui.ctx(),
                        s.clone(),
                        DragMode::Move,
                        start,
                        page_rect,
                        page_size,
                    );
                }
            }
        }
        if s.is_image || self.tool == Tool::Move {
            let recognize =
                s.is_image && self.workspace == Workspace::Edit && self.tool == Tool::Edit;
            let response = ui.interact(
                outline.shrink(7.),
                egui::Id::new(("move-body", s.page)),
                if recognize {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::drag()
                },
            );
            // Once selected, the image owns pointer events above the page. Handle
            // its double-click here as well, while keeping ordinary dragging.
            if recognize && response.double_clicked() && self.drag.is_none() {
                self.start_automatic_recognition(ui.ctx(), s);
                return;
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(moving_cursor(ui.ctx()));
            }
            if response.drag_started() {
                if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                    self.begin_drag(ui.ctx(), s, DragMode::Move, start, page_rect, page_size);
                }
            }
        }
    }
    fn begin_drag(
        &mut self,
        ctx: &egui::Context,
        s: Selection,
        mode: DragMode,
        start: Pos2,
        _page_rect: Rect,
        size: Vec2,
    ) {
        let editing = self
            .inline
            .as_ref()
            .is_some_and(|i| i.selection.page == s.page && i.selection.target == s.target);
        if !self.finish_text() || !self.permit() {
            return;
        }
        // Formatting may turn an added text object into a native rich block.
        // Follow the committed selection, not an index that now names a neighbour.
        let s = if editing {
            self.selected.clone().unwrap_or(s)
        } else {
            self.selection_for(s.page, s.target).unwrap_or(s)
        };
        let Some(e) = &self.editor else { return };
        let Some(p) = e.info.pages.get(s.page) else {
            return;
        };
        let source = s.view_rect(p);
        let text_preview = if !s.is_image
            && !matches!(s.target, Target::New)
            && matches!(mode, DragMode::Corner(_))
        {
            match inline::Inline::new(
                ctx,
                e,
                s.clone(),
                (self.zoom * ctx.pixels_per_point()).clamp(0.25, 4.),
            ) {
                Ok(mut preview) => {
                    preview.reflow = true;
                    Some(preview)
                }
                Err(error) => {
                    self.message = Some(error);
                    return;
                }
            }
        } else {
            None
        };
        let bytes = match s.target {
            Target::Image(index) => pdfinator::document::guarded(|| {
                let mut doc = e.doc.clone();
                pdfcraft_edit::change_image(
                    &mut doc,
                    s.page,
                    index,
                    &pdfcraft_edit::ImageChange::Delete,
                )
                .map_err(|e| e.to_string())?;
                pdfcraft_cos::write_incremental(&doc, &pdfcraft_cos::SaveOptions::default())
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            }),
            _ => e.preview_without(
                s.page,
                match s.target {
                    Target::Block(i) => Some(i),
                    _ => None,
                },
                match s.target {
                    Target::Added(i) => Some(i),
                    _ => None,
                },
            ),
        };
        match bytes {
            Ok(bytes) => {
                self.drag = Some(Drag {
                    selection: s.clone(),
                    mode,
                    start,
                    source,
                    current: source,
                    background: raster::Background::new(
                        bytes,
                        s.page,
                        (self.zoom * ctx.pixels_per_point()).clamp(0.25, 4.),
                    ),
                    texture: if matches!(s.target, Target::New) {
                        None
                    } else {
                        self.raster.frames.get(&s.page).map(|f| f.texture.clone())
                    },
                    page_size: size,
                    committed: false,
                    text_preview,
                });
                self.inline = None;
                self.selected = Some(s);
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("direct-text")));
            }
            Err(e) => self.message = Some(e),
        }
    }
    fn update_drag(&mut self, ui: &egui::Ui, _page_rect: Rect, page: usize) {
        let Some(d) = &mut self.drag else { return };
        if d.selection.page != page || d.committed {
            return;
        }
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let delta = (pos - d.start) / self.zoom;
            d.current = drag_rect(
                d.source,
                d.mode,
                delta,
                d.selection.is_image && self.lock_aspect,
            );
            if let Some(preview) = &d.text_preview
                && let Some(galley) = preview.layout(
                    ui,
                    &d.selection.draft,
                    self.zoom,
                    d.current.width().max(4.) * self.zoom,
                )
            {
                d.current.max.y = d.current.min.y + galley.size().y / self.zoom;
            }
        }
        if ui.input(|i| {
            i.pointer.button_released(egui::PointerButton::Primary) || !i.pointer.primary_down()
        }) {
            let Some(mut drag) = self.drag.take() else {
                return;
            };
            let before = self.editor.as_ref().map(Editor::revision);
            self.transform(drag.selection.clone(), drag.current);
            if matches!(drag.mode, DragMode::Create) {
                if let Some(selection) = self.selected.clone() {
                    self.begin_text(ui.ctx(), selection);
                }
            }
            if self.editor.as_ref().map(Editor::revision) != before {
                drag.committed = true;
                self.drag = Some(drag);
            }
        }
    }
    fn place_image(&mut self, page: usize, p: &PageInfo, point: Pos2) {
        let Some(path) = self.image_path.clone() else {
            return;
        };
        if !self.permit() {
            return;
        }
        let bytes = match pdfinator::files::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        let mut scratch = pdfcraft_cos::Document::new_empty();
        let natural =
            match pdfcraft_create::image_xobject(&mut scratch, &path.to_string_lossy(), &bytes) {
                Ok((_, size)) => size,
                Err(e) => {
                    self.message = Some(e.to_string());
                    return;
                }
            };
        let w = 180.0_f64.min((p.width - point.x).max(20.) as f64);
        let h = w * natural.1 / natural.0.max(0.01);
        let x = point.x as f64;
        let y = (p.height - point.y) as f64;
        if let Some(e) = &mut self.editor {
            let index = e.added(page).len();
            let result = e.image_bytes(
                page,
                [x, y - h, x + w, y],
                &path.to_string_lossy(),
                &bytes,
                None,
            );
            if self.changed(result, page) {
                self.tool = Tool::Edit;
                self.selected = self.selection_for(page, Target::Added(index));
            }
        }
    }
}
/// Page gaps remain in screen points while page contents scale.
fn zoom_offset(
    offset: Vec2,
    pointer: Vec2,
    width: f32,
    old: f32,
    new: f32,
    pages: &[Vec2],
) -> Vec2 {
    let y = offset.y + pointer.y;
    let mut old_top = 0.;
    let mut new_top = 0.;
    for p in pages {
        if y <= old_top + p.y * old + 20. {
            let within = (y - old_top).min(p.y * old);
            let gap = (y - old_top - p.y * old).max(0.);
            let old_left = ((width - 18. - p.x * old) / 2.).max(0.);
            let new_left = ((width - 18. - p.x * new) / 2.).max(0.);
            return Vec2::new(
                (new_left + (offset.x + pointer.x - old_left) * new / old - pointer.x).max(0.),
                (new_top + within * new / old + gap - pointer.y).max(0.),
            );
        }
        old_top += p.y * old + 20.;
        new_top += p.y * new + 20.;
    }
    offset * new / old
}
pub(super) fn drag_rect(source: Rect, mode: DragMode, delta: Vec2, proportional: bool) -> Rect {
    match mode {
        DragMode::Create => {
            let r = Rect::from_two_pos(source.min, source.min + delta);
            Rect::from_min_size(r.min, r.size().max(Vec2::splat(8.)))
        }
        DragMode::Move => source.translate(delta),
        DragMode::Corner(corner) => {
            let left = corner == 0 || corner == 3;
            let top = corner == 0 || corner == 1;
            let mut w = (source.width() + if left { -delta.x } else { delta.x }).max(8.);
            let mut h = (source.height() + if top { -delta.y } else { delta.y }).max(8.);
            if proportional {
                let factor = if delta.x.abs() / source.width().max(1.)
                    >= delta.y.abs() / source.height().max(1.)
                {
                    w / source.width().max(1.)
                } else {
                    h / source.height().max(1.)
                };
                w = source.width() * factor;
                h = source.height() * factor;
            }
            let x = if left {
                source.right() - w
            } else {
                source.left()
            };
            let y = if top {
                source.bottom() - h
            } else {
                source.top()
            };
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_keeps_the_same_pdf_point_under_pointer_after_page_gaps() {
        let pages = [Vec2::new(595., 842.); 3];
        let pointer = Vec2::new(300., 200.);
        let offset = Vec2::new(0., 1000.);
        let next = zoom_offset(offset, pointer, 800., 1., 2., &pages);
        // Page 2 starts at y=862, the pointer is 338pt into its contents.
        assert!((next.y + pointer.y - (842. * 2. + 20. + 338. * 2.)).abs() < 0.01);
        let old_left = (800. - 18. - 595.) / 2.;
        assert!(((next.x + pointer.x) / 2. - (pointer.x - old_left)).abs() < 0.01);
        let back = zoom_offset(next, pointer, 800., 2., 1., &pages);
        assert!((back - offset).length() < 0.01);
    }
    #[test]
    fn image_resize_preserves_ratio_and_opposite_corner() {
        let r = Rect::from_min_size(Pos2::new(10., 20.), Vec2::new(200., 100.));
        for corner in 0..4 {
            let n = drag_rect(r, DragMode::Corner(corner), Vec2::new(37., 19.), true);
            assert!((n.width() / n.height() - 2.).abs() < 0.001);
            let opposite = (corner + 2) % 4;
            let old = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ][opposite];
            let new = [
                n.left_top(),
                n.right_top(),
                n.right_bottom(),
                n.left_bottom(),
            ][opposite];
            assert_eq!(old, new);
        }
    }
}
