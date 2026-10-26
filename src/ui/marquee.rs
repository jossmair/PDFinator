//! Document-space rectangular selection, with one history step for a mixed move.
use super::*;
use pdfinator::selection::{Item, Target as MixedTarget};
use std::sync::mpsc::{self, Receiver};

#[derive(Default)]
pub(super) struct State {
    pub selected: Vec<Item>,
    pub page: usize,
    cache: HashMap<usize, (Arc<Vec<u8>>, Vec<Item>)>,
    gesture: Option<Gesture>,
    preview: Option<Preview>,
}
enum Gesture {
    Box {
        start: Pos2,
        current: Pos2,
        add: bool,
    },
    Move {
        start: Pos2,
        delta: Vec2,
    },
}
type PreviewResult = Result<(pdfcraft_render::RenderedPage, Vec<u8>)>;
struct Preview {
    rx: Option<Receiver<PreviewResult>>,
    background: Option<egui::TextureHandle>,
    layer: Option<egui::TextureHandle>,
    offset: Vec2,
    committed: bool,
}
fn bounds(item: &Item, p: &PageInfo) -> Rect {
    let a = p.user_to_view(item.rect[0] as f32, item.rect[1] as f32);
    let b = p.user_to_view(item.rect[2] as f32, item.rect[3] as f32);
    Rect::from_two_pos(Pos2::new(a[0], a[1]), Pos2::new(b[0], b[1]))
}
impl State {
    #[cfg(test)]
    pub fn preview_ready(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|p| p.background.is_some() && p.layer.is_some())
    }
    pub fn clear(&mut self) {
        self.selected.clear();
        self.gesture = None;
        self.preview = None;
    }
    pub fn page_texture(&self, page: usize) -> Option<&egui::TextureHandle> {
        (self.page == page
            && (self
                .gesture
                .as_ref()
                .is_some_and(|g| matches!(g, Gesture::Move { .. }))
                || self.preview.as_ref().is_some_and(|p| p.committed)))
        .then(|| self.preview.as_ref()?.background.as_ref())
        .flatten()
    }
    pub fn poll(&mut self, ctx: &egui::Context, current: bool) -> Option<String> {
        let preview = self.preview.as_mut()?;
        if preview.committed && current {
            self.preview = None;
            return None;
        }
        if let Some(result) = preview.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            preview.rx = None;
            match result {
                Ok((background, rgba)) => {
                    let size = [background.width as usize, background.height as usize];
                    preview.background = Some(ctx.load_texture(
                        "mixed-background",
                        egui::ColorImage::from_rgba_premultiplied(size, &background.rgba),
                        Default::default(),
                    ));
                    preview.layer = Some(ctx.load_texture(
                        "mixed-layer",
                        egui::ColorImage::from_rgba_premultiplied(size, &rgba),
                        Default::default(),
                    ));
                }
                Err(e) => return Some(e),
            }
        }
        if preview.rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        None
    }
}
impl App {
    fn mixed_inventory(&mut self, page: usize) -> Vec<Item> {
        let Some(e) = &self.editor else {
            return Vec::new();
        };
        if self
            .marquee
            .cache
            .get(&page)
            .is_none_or(|(bytes, _)| !Arc::ptr_eq(bytes, &e.bytes))
        {
            match e.selection_items(page) {
                Ok(items) => {
                    self.marquee.cache.insert(page, (e.bytes.clone(), items));
                }
                Err(error) => {
                    self.message = Some(error);
                    return Vec::new();
                }
            }
        }
        self.marquee.cache[&page].1.clone()
    }
    fn mixed_preview(&mut self, ctx: &egui::Context) {
        self.marquee.preview = None;
        if self.marquee.selected.is_empty() {
            return;
        }
        let page = self.marquee.page;
        let targets = self
            .marquee
            .selected
            .iter()
            .map(|i| i.target)
            .collect::<Vec<_>>();
        let Some(e) = &self.editor else {
            return;
        };
        let doc = e.doc.clone();
        let scale = (self.zoom * ctx.pixels_per_point()).clamp(0.5, 2.);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.marquee.preview = Some(Preview {
            rx: Some(rx),
            background: None,
            layer: None,
            offset: Vec2::ZERO,
            committed: false,
        });
        std::thread::spawn(move || {
            let result = pdfinator::document::guarded(|| {
                let e = Editor::from_bytes(
                    pdfcraft_cos::write_full(&doc, &Default::default())
                        .map_err(|e| e.to_string())?,
                )?;
                let render = |bytes| -> Result<_> {
                    let mut renderer =
                        pdfcraft_render::PageRenderer::new(bytes, Default::default());
                    let r = renderer.render(pdfcraft_render::RenderRequest {
                        page,
                        scale,
                        ..Default::default()
                    });
                    if let Some(error) = &r.error {
                        return Err(error.clone());
                    }
                    Ok(r)
                };
                let background = render(e.selection_preview(page, &targets, false, false)?)?;
                let white = render(e.selection_preview(page, &targets, true, false)?)?;
                let black = render(e.selection_preview(page, &targets, true, true)?)?;
                if white.rgba.len() != black.rgba.len() || white.rgba.len() != background.rgba.len()
                {
                    return Err("Aperçu de sélection indisponible.".into());
                }
                let mut rgba = black.rgba.to_vec();
                for (b, w) in rgba
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(white.rgba.as_chunks::<4>().0)
                {
                    let transparency = (0..3)
                        .map(|c| w[c].saturating_sub(b[c]))
                        .max()
                        .unwrap_or(255);
                    b[3] = 255 - transparency;
                    for c in &mut b[..3] {
                        *c = (*c).min(255 - transparency);
                    }
                }
                Ok((background, rgba))
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn sync_image_selection(&mut self, page: usize) {
        self.selected = self
            .marquee
            .selected
            .first()
            .filter(|_| self.marquee.selected.len() == 1)
            .and_then(|i| {
                let target = match i.target {
                    MixedTarget::Object(t) if t.kind == ObjectKind::Image => Target::Image(t.index),
                    MixedTarget::Object(t) if t.kind == ObjectKind::Added => Target::Added(t.index),
                    _ => return None,
                };
                self.selection_for(page, target).filter(|s| s.is_image)
            });
    }
    fn mixed_move(&mut self, ctx: &egui::Context, delta: Vec2) -> bool {
        if delta.length_sq() < 0.0001 || !self.permit() {
            return false;
        }
        let page = self.marquee.page;
        let Some(p) = self
            .editor
            .as_ref()
            .and_then(|e| e.info.pages.get(page))
            .cloned()
        else {
            return false;
        };
        let a = p.view_to_user(0., 0.);
        let b = p.view_to_user(delta.x, delta.y);
        let user = [(b[0] - a[0]) as f64, (b[1] - a[1]) as f64];
        let selected = self.marquee.selected.clone();
        let targets = selected.iter().map(|i| i.target).collect::<Vec<_>>();
        let result = self
            .editor
            .as_mut()
            .ok_or("Document absent.".to_string())
            .and_then(|e| e.move_selection(page, &targets, user));
        if !self.changed(result, page) {
            return false;
        }
        let inventory = self.mixed_inventory(page);
        self.marquee.selected = selected
            .iter()
            .filter_map(|old| {
                let rect = [
                    old.rect[0] + user[0],
                    old.rect[1] + user[1],
                    old.rect[2] + user[0],
                    old.rect[3] + user[1],
                ];
                inventory
                    .iter()
                    .find(|i| i.rect.iter().zip(rect).all(|(a, b)| (*a - b).abs() < 0.2))
                    .cloned()
            })
            .collect();
        self.sync_image_selection(page);
        if let Some(preview) = &mut self.marquee.preview {
            preview.offset = delta;
            preview.committed = true;
        }
        ctx.request_repaint();
        true
    }
    pub(super) fn mixed_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.image_crop.is_some() {
            return false;
        }
        if self.workspace != Workspace::Edit || !matches!(self.tool, Tool::Select | Tool::Edit) {
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.marquee.page = self.page;
            self.marquee.selected = self.mixed_inventory(self.page);
            self.sync_image_selection(self.page);
            self.mixed_preview(ctx);
            return true;
        }
        if self.marquee.selected.is_empty() {
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.marquee.clear();
            self.selected = None;
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            if self.permit() {
                let page = self.marquee.page;
                let targets = self
                    .marquee
                    .selected
                    .iter()
                    .map(|i| i.target)
                    .collect::<Vec<_>>();
                let result = self
                    .editor
                    .as_mut()
                    .ok_or("Document absent.".to_string())
                    .and_then(|e| e.delete_selection(page, &targets));
                if self.changed(result, page) {
                    self.marquee.clear();
                    self.selected = None;
                }
            }
            return true;
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
            if ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, key) || i.consume_key(Modifiers::SHIFT, key)
            }) {
                if self.marquee.preview.as_ref().is_some_and(|p| p.committed) {
                    self.marquee.preview = None;
                }
                self.mixed_move(ctx, delta);
                self.mixed_preview(ctx);
                return true;
            }
        }
        false
    }
    pub(super) fn mixed_edit_capture(
        &self,
        ui: &egui::Ui,
        page: usize,
        p: &PageInfo,
        sheet: Rect,
        response: &egui::Response,
    ) -> bool {
        if self.drag.is_some() || !self.drawing.selected.is_empty() {
            return false;
        }
        if self.marquee.page == page
            && (self.marquee.gesture.is_some()
                || (!self.marquee.selected.is_empty() && !response.clicked()))
        {
            return true;
        }
        let Some(start) = ui.input(|i| i.pointer.press_origin()) else {
            return false;
        };
        // Character selection and the selected block's border/resize handles
        // keep their own gestures. Everywhere else, dragging selects objects.
        if self.inline.as_ref().is_some_and(|i| {
            !i.committed && i.selection.page == page && i.screen.expand(9.).contains(start)
        }) || self.selected.as_ref().is_some_and(|s| {
            s.page == page
                && Rect::from_min_max(
                    sheet.min + s.view_rect(p).min.to_vec2() * self.zoom,
                    sheet.min + s.view_rect(p).max.to_vec2() * self.zoom,
                )
                .expand(9.)
                .contains(start)
        }) {
            return false;
        }
        response.drag_started_by(egui::PointerButton::Primary)
            || (self.canvas.contains(start)
                && !sheet.contains(start)
                && start.y >= sheet.top()
                && start.y <= sheet.bottom()
                && ui.input(|i| {
                    i.pointer.primary_down()
                        && i.pointer
                            .interact_pos()
                            .is_some_and(|pos| start.distance(pos) > 6.)
                }))
    }
    pub(super) fn mixed_canvas(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        p: &PageInfo,
        sheet: Rect,
        response: &egui::Response,
    ) {
        let all = self.mixed_inventory(page);
        let zoom = self.zoom;
        let local = |pos: Pos2| {
            let v = (pos - sheet.min) / zoom;
            Pos2::new(v.x, v.y)
        };
        let screen = |r: Rect| {
            Rect::from_min_max(
                sheet.min + r.min.to_vec2() * zoom,
                sheet.min + r.max.to_vec2() * zoom,
            )
        };
        let group = self
            .marquee
            .selected
            .iter()
            .fold(Rect::NOTHING, |r, i| r.union(bounds(i, p)));
        if self.marquee.page == page
            && let Some(preview) = &self.marquee.preview
            && self.marquee.page_texture(page).is_some()
            && let Some(texture) = &preview.layer
        {
            let delta = match self.marquee.gesture {
                Some(Gesture::Move { delta, .. }) => delta,
                _ => preview.offset,
            };
            ui.painter().image(
                texture.id(),
                sheet.translate(delta * zoom),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        let from_margin = self.marquee.gesture.is_none()
            && ui.input(|i| {
                i.pointer.primary_down()
                    && i.pointer
                        .press_origin()
                        .zip(i.pointer.interact_pos())
                        .is_some_and(|(start, current)| {
                            self.canvas.contains(start)
                                && !sheet.contains(start)
                                && start.y >= sheet.top()
                                && start.y <= sheet.bottom()
                                && start.distance(current) > 6.
                        })
            });
        if (response.drag_started_by(egui::PointerButton::Primary) || from_margin)
            && let Some(start) = ui.input(|i| i.pointer.press_origin())
        {
            let start = local(start);
            if self.marquee.page == page
                && group.expand(5. / zoom).contains(start)
                && !self.marquee.selected.is_empty()
            {
                if self.marquee.preview.as_ref().is_none_or(|p| p.committed) {
                    self.mixed_preview(ui.ctx());
                }
                self.marquee.gesture = Some(Gesture::Move {
                    start,
                    delta: Vec2::ZERO,
                });
            } else {
                let add = ui.input(|i| i.modifiers.shift);
                if !add || self.marquee.page != page {
                    self.marquee.clear();
                }
                self.marquee.page = page;
                self.marquee.gesture = Some(Gesture::Box {
                    start,
                    current: start,
                    add,
                });
            }
            self.selected = None;
            self.inline = None;
            self.drawing.selected.clear();
            self.text_selection = None;
            self.close_search();
        }
        if self.marquee.page == page
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            match &mut self.marquee.gesture {
                Some(Gesture::Box { current, .. }) => *current = local(pos),
                Some(Gesture::Move { start, delta }) => *delta = local(pos) - *start,
                None => {}
            }
        }
        if self.marquee.page == page {
            if let Some(Gesture::Box { start, current, .. }) = self.marquee.gesture {
                let area = screen(Rect::from_two_pos(start, current));
                ui.painter().rect_filled(
                    area,
                    2.,
                    Color32::from_rgba_unmultiplied(60, 125, 255, 25),
                );
                ui.painter().rect_stroke(
                    area,
                    2.,
                    Stroke::new(1.3, icons::BLUE),
                    StrokeKind::Outside,
                );
            }
            let delta = match self.marquee.gesture {
                Some(Gesture::Move { delta, .. }) => delta,
                _ => Vec2::ZERO,
            };
            for item in &self.marquee.selected {
                ui.painter().rect_stroke(
                    screen(bounds(item, p).translate(delta)),
                    1.,
                    Stroke::new(0.8, Color32::from_rgba_unmultiplied(60, 125, 255, 120)),
                    StrokeKind::Outside,
                );
            }
            if !self.marquee.selected.is_empty() {
                let r = screen(group.translate(delta)).expand(3.);
                ui.painter()
                    .rect_stroke(r, 2., Stroke::new(1.5, icons::BLUE), StrokeKind::Outside);
                for point in [
                    r.left_top(),
                    r.right_top(),
                    r.left_bottom(),
                    r.right_bottom(),
                ] {
                    ui.painter().circle_filled(point, 3., Color32::WHITE);
                    ui.painter()
                        .circle_stroke(point, 3., Stroke::new(1.2, icons::BLUE));
                }
                if response.hovered()
                    && ui
                        .ctx()
                        .pointer_hover_pos()
                        .is_some_and(|pos| r.contains(pos))
                {
                    ui.ctx().set_cursor_icon(moving_cursor(ui.ctx()));
                }
            }
            if self.marquee.gesture.is_some()
                && (response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()))
            {
                match self.marquee.gesture.take().unwrap() {
                    Gesture::Box {
                        start,
                        current,
                        add,
                    } => {
                        let area = Rect::from_two_pos(start, current).expand(0.5 / zoom);
                        if !add {
                            self.marquee.selected.clear();
                        }
                        for item in all.iter().filter(|i| area.contains_rect(bounds(i, p))) {
                            if !self
                                .marquee
                                .selected
                                .iter()
                                .any(|i| i.target == item.target)
                            {
                                self.marquee.selected.push(item.clone());
                            }
                        }
                        self.sync_image_selection(page);
                        self.mixed_preview(ui.ctx());
                    }
                    Gesture::Move { delta, .. } => {
                        if !self.mixed_move(ui.ctx(), delta) {
                            self.marquee.preview = None;
                        }
                    }
                }
            }
        }
        if self.tool == Tool::Select
            && response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let point = local(pos);
            let hit = all
                .iter()
                .filter(|i| bounds(i, p).expand(3. / zoom).contains(point))
                .min_by(|a, b| {
                    let priority = |i: &Item| {
                        if matches!(
                            i.target,
                            MixedTarget::Object(pdfcraft_edit::ObjectTarget {
                                kind: ObjectKind::Text,
                                ..
                            })
                        ) {
                            0
                        } else {
                            1
                        }
                    };
                    priority(a)
                        .cmp(&priority(b))
                        .then_with(|| bounds(a, p).area().total_cmp(&bounds(b, p).area()))
                })
                .cloned();
            if !ui.input(|i| i.modifiers.shift) {
                self.marquee.clear();
            }
            self.marquee.page = page;
            if let Some(item) = hit {
                if let Some(index) = self
                    .marquee
                    .selected
                    .iter()
                    .position(|i| i.target == item.target)
                {
                    self.marquee.selected.remove(index);
                } else {
                    self.marquee.selected.push(item);
                }
            }
            self.sync_image_selection(page);
            self.text_selection = None;
            self.mixed_preview(ui.ctx());
        }
    }
}
