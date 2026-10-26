use super::*;

pub(super) struct State {
    pub selection: Selection,
    source: Rect,
    area: Rect,
    bytes: Arc<Vec<u8>>,
    drag: Option<(usize, Pos2, Rect)>,
}
impl State {
    fn margins(&self) -> [f64; 4] {
        [
            (self.area.left() - self.source.left()) / self.source.width(),
            (self.area.top() - self.source.top()) / self.source.height(),
            (self.source.right() - self.area.right()) / self.source.width(),
            (self.source.bottom() - self.area.bottom()) / self.source.height(),
        ]
        .map(|v| v as f64 * 100.)
    }
}
impl App {
    pub(super) fn begin_image_crop(&mut self, ctx: &egui::Context, selection: Selection) {
        if !selection.is_image || !self.deselect() {
            return;
        }
        let Some(e) = &self.editor else { return };
        let Some(p) = e.info.pages.get(selection.page) else {
            return;
        };
        let source = selection.view_rect(p);
        self.image_crop = Some(State {
            selection: selection.clone(),
            source,
            area: source,
            bytes: e.bytes.clone(),
            drag: None,
        });
        self.inline = None;
        self.selected = Some(selection);
        self.tool = Tool::Select;
        self.properties_collapsed = false;
        ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("direct-text")));
    }
    fn apply_image_crop(&mut self) {
        if !self.permit() {
            return;
        }
        let Some(crop) = self.image_crop.take() else {
            return;
        };
        let source = match crop.selection.target {
            Target::Image(i) => pdfinator::ocr::ImageSource::Original(i),
            Target::Added(i) => pdfinator::ocr::ImageSource::Added(i),
            _ => return,
        };
        let Some(e) = &mut self.editor else { return };
        if !Arc::ptr_eq(&e.bytes, &crop.bytes) {
            self.message = Some("L’image a changé. Sélectionnez-la de nouveau.".into());
            return;
        }
        let result = e.crop_image(crop.selection.page, source, crop.margins());
        if self.changed(result, crop.selection.page) {
            self.selected = None;
            self.marquee.clear();
        }
    }
    pub(super) fn image_crop_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Rogner l’image");
        ui.add_space(12.);
        if let Some(crop) = &mut self.image_crop {
            let mut margins = crop.margins();
            let mut changed = false;
            for (i, label) in ["Gauche", "Haut", "Droite", "Bas"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    let r = ui.add(
                        egui::DragValue::new(&mut margins[i])
                            .range(0.0..=99.0)
                            .suffix(" %")
                            .max_decimals(2),
                    );
                    changed |= r.changed();
                    r.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::DragValue,
                            true,
                            format!("Rognage {label}"),
                        )
                    });
                });
            }
            if changed && margins[0] + margins[2] < 100. && margins[1] + margins[3] < 100. {
                crop.area = Rect::from_min_max(
                    crop.source.min
                        + crop.source.size() * Vec2::new(margins[0] as f32, margins[1] as f32)
                            / 100.,
                    crop.source.max
                        - crop.source.size() * Vec2::new(margins[2] as f32, margins[3] as f32)
                            / 100.,
                );
            }
        }
        ui.add_space(12.);
        if ui.button("Appliquer le rognage").clicked() {
            self.apply_image_crop();
        }
        if ui.button("Annuler le rognage").clicked() {
            self.image_crop = None;
        }
    }
    pub(super) fn image_crop_canvas(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        sheet: Rect,
    ) -> bool {
        if !self
            .image_crop
            .as_ref()
            .is_some_and(|c| c.selection.page == page)
        {
            return false;
        }
        let mut crop = self.image_crop.take().unwrap();
        let zoom = self.zoom;
        let screen = |r: Rect| {
            Rect::from_min_max(
                sheet.min + r.min.to_vec2() * zoom,
                sheet.min + r.max.to_vec2() * zoom,
            )
        };
        let local = |p: Pos2| {
            let v = (p - sheet.min) / zoom;
            Pos2::new(v.x, v.y)
        };
        let source = screen(crop.source);
        let area = screen(crop.area);
        let body = ui.interact(
            source,
            egui::Id::new(("crop-body", page)),
            egui::Sense::click_and_drag(),
        );
        if body.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            let pos = local(pos);
            let mode = if crop.area != crop.source && crop.area.contains(pos) {
                9
            } else {
                8
            };
            crop.drag = Some((mode, pos, crop.area));
        }
        for r in [
            Rect::from_min_max(source.min, Pos2::new(source.right(), area.top())),
            Rect::from_min_max(Pos2::new(source.left(), area.bottom()), source.max),
            Rect::from_min_max(
                Pos2::new(source.left(), area.top()),
                Pos2::new(area.left(), area.bottom()),
            ),
            Rect::from_min_max(
                Pos2::new(area.right(), area.top()),
                Pos2::new(source.right(), area.bottom()),
            ),
        ] {
            if r.is_positive() {
                ui.painter()
                    .rect_filled(r, 0., Color32::from_black_alpha(115));
            }
        }
        ui.painter().rect_stroke(
            area,
            0.,
            Stroke::new(2., Color32::WHITE),
            StrokeKind::Inside,
        );
        let handles = [
            area.left_top(),
            area.right_top(),
            area.right_bottom(),
            area.left_bottom(),
            area.left_center(),
            area.center_top(),
            area.right_center(),
            area.center_bottom(),
        ];
        for (index, point) in handles.into_iter().enumerate() {
            let r = Rect::from_center_size(point, Vec2::splat(14.));
            let response = ui.interact(
                r,
                egui::Id::new(("crop-handle", page, index)),
                egui::Sense::click_and_drag(),
            );
            ui.painter().rect_filled(
                Rect::from_center_size(point, Vec2::splat(8.)),
                0.,
                Color32::BLACK,
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(match index {
                    0 | 2 => egui::CursorIcon::ResizeNwSe,
                    1 | 3 => egui::CursorIcon::ResizeNeSw,
                    4 | 6 => egui::CursorIcon::ResizeHorizontal,
                    _ => egui::CursorIcon::ResizeVertical,
                });
            }
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(pos) = ui.input(|i| i.pointer.press_origin())
            {
                crop.drag = Some((index, local(pos), crop.area));
            }
        }
        if let Some((index, start, old)) = crop.drag
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            let pos = local(pos);
            let delta = pos - start;
            let mut next = old;
            let min = 2.0_f32
                .min(crop.source.width() / 2.)
                .min(crop.source.height() / 2.)
                .min(old.width())
                .min(old.height());
            match index {
                8 => next = Rect::from_two_pos(start, pos).intersect(crop.source),
                9 => {
                    let dx = delta.x.clamp(
                        crop.source.left() - old.left(),
                        crop.source.right() - old.right(),
                    );
                    let dy = delta.y.clamp(
                        crop.source.top() - old.top(),
                        crop.source.bottom() - old.bottom(),
                    );
                    next = old.translate(Vec2::new(dx, dy));
                }
                _ => {
                    if matches!(index, 0 | 3 | 4) {
                        next.min.x =
                            (old.min.x + delta.x).clamp(crop.source.min.x, old.max.x - min);
                    }
                    if matches!(index, 1 | 2 | 6) {
                        next.max.x =
                            (old.max.x + delta.x).clamp(old.min.x + min, crop.source.max.x);
                    }
                    if matches!(index, 0 | 1 | 5) {
                        next.min.y =
                            (old.min.y + delta.y).clamp(crop.source.min.y, old.max.y - min);
                    }
                    if matches!(index, 2 | 3 | 7) {
                        next.max.y =
                            (old.max.y + delta.y).clamp(old.min.y + min, crop.source.max.y);
                    }
                }
            }
            if next.width() >= min && next.height() >= min {
                crop.area = next;
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                crop.drag = None;
            }
            ui.ctx().request_repaint();
        }
        self.image_crop = Some(crop);
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.image_crop = None;
        } else if !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            self.apply_image_crop();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_keep_extreme_numeric_crops_valid() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        for handle in 0..8 {
            let source = Rect::from_min_max(Pos2::ZERO, Pos2::new(100., 100.));
            let area = Rect::from_min_max(Pos2::new(99., 99.), source.max);
            let mut selection = Selection::text(
                0,
                Target::Image(0),
                [0., 0., 100., 100.],
                String::new(),
                12.,
                [0.; 3],
                false,
            );
            selection.is_image = true;
            app.image_crop = Some(State {
                selection,
                source,
                area,
                bytes: Arc::new(Vec::new()),
                drag: Some((handle, area.max, area)),
            });
            let pos = Pos2::new(200., 200.);
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    app.image_crop_canvas(ui, 0, source);
                },
            );
            out.textures_delta.clear();
            let result = app.image_crop.as_ref().unwrap().area;
            assert!(
                result.is_positive() && result.is_finite() && source.contains_rect(result),
                "handle {handle}: {result:?}"
            );
        }
    }
}
