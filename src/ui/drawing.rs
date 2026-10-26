use super::*;
use pdfinator::drawing::{Drawing, Item, Kind, StrokeStyle, Target as DrawingTarget};
pub(super) struct State {
    pub selected: Vec<Item>,
    pub page: usize,
    pub mode: Option<Kind>,
    pub square: bool,
    width: f64,
    color: [f32; 3],
    style: StrokeStyle,
    rows: u16,
    columns: u16,
    all: bool,
    linked_text: bool,
    inventory: std::collections::HashMap<usize, (Arc<Vec<u8>>, Vec<Item>)>,
    pub drag: Option<Gesture>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            page: 0,
            mode: None,
            square: false,
            width: 1.5,
            color: [0.1, 0.2, 0.35],
            style: StrokeStyle::Solid,
            rows: 4,
            columns: 4,
            all: false,
            linked_text: false,
            inventory: Default::default(),
            drag: None,
        }
    }
}
pub(super) struct Gesture {
    pub page: usize,
    pub panel_visible: bool,
    start: Pos2,
    original: Vec<Item>,
    draft: Vec<Drawing>,
    kind: Action,
    pub background: Option<raster::Background>,
    pub committed: bool,
}
#[derive(Clone, Copy)]
enum Action {
    New,
    Move,
    Corner(usize),
}
fn bounds(d: &Drawing, p: &PageInfo) -> Rect {
    d.view_points(p).into_iter().fold(Rect::NOTHING, |r, p| {
        r.union(Rect::from_min_size(Pos2::new(p[0], p[1]), Vec2::ZERO))
    })
}
fn user_bounds(items: &[Item]) -> [f64; 4] {
    items
        .iter()
        .flat_map(|i| {
            [
                i.drawing.at(0., 0.),
                i.drawing.at(1., 0.),
                i.drawing.at(1., 1.),
                i.drawing.at(0., 1.),
            ]
        })
        .fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |r, p| {
                [
                    r[0].min(p[0]),
                    r[1].min(p[1]),
                    r[2].max(p[0]),
                    r[3].max(p[1]),
                ]
            },
        )
}
fn distance(item: &Item, p: &PageInfo, point: Pos2) -> f32 {
    if item.drawing.kind == Kind::FilledRectangle && bounds(&item.drawing, p).contains(point) {
        return 0.;
    }
    let segment = |a: [f32; 2], b: [f32; 2]| {
        let a = Pos2::new(a[0], a[1]);
        let b = Pos2::new(b[0], b[1]);
        let ab = b - a;
        let t = ((point - a).dot(ab) / ab.length_sq().max(0.01)).clamp(0., 1.);
        point.distance(a + ab * t)
    };
    let mut points = if item.drawing.kind == Kind::Circle {
        (0..=96)
            .map(|n| {
                let t = n as f64 / 96. * std::f64::consts::TAU;
                let a = item.drawing.at(0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin());
                p.user_to_view(a[0] as f32, a[1] as f32)
            })
            .collect::<Vec<_>>()
    } else {
        item.drawing.view_points(p)
    };
    if !matches!(item.drawing.kind, Kind::Line | Kind::Circle) {
        points.push(points[0]);
    }
    points
        .windows(2)
        .map(|s| segment(s[0], s[1]))
        .fold(f32::INFINITY, f32::min)
}
fn hit(item: &Item, p: &PageInfo, point: Pos2, tolerance: f32) -> bool {
    distance(item, p, point) < tolerance + item.drawing.view_width(p) as f32 / 2.
}
fn closest(items: &[Item], p: &PageInfo, point: Pos2, tolerance: f32) -> Option<Item> {
    let fill_size = items
        .iter()
        .filter(|i| {
            i.drawing.kind == Kind::FilledRectangle && bounds(&i.drawing, p).contains(point)
        })
        .map(|i| {
            let b = bounds(&i.drawing, p);
            b.width().min(b.height())
        })
        .reduce(f32::min);
    let priority = |i: &Item| {
        if i.drawing.kind == Kind::FilledRectangle {
            1
        } else if fill_size.is_some_and(|size| {
            distance(i, p, point)
                > (tolerance * 0.5).min(size * 0.25) + i.drawing.view_width(p) as f32 / 2.
        }) {
            2
        } else {
            0
        }
    };
    items
        .iter()
        .filter(|i| hit(i, p, point, tolerance))
        .min_by(|a, b| {
            priority(a)
                .cmp(&priority(b))
                .then_with(|| distance(a, p, point).total_cmp(&distance(b, p, point)))
        })
        .cloned()
}
fn paint(ui: &egui::Ui, d: &Drawing, p: &PageInfo, page_rect: Rect, zoom: f32) {
    let pos = |x, y| {
        let v = d.at(x, y);
        let q = p.user_to_view(v[0] as f32, v[1] as f32);
        page_rect.min + Vec2::new(q[0], q[1]) * zoom
    };
    let color = Color32::from_rgb(
        (d.color[0] * 255.) as u8,
        (d.color[1] * 255.) as u8,
        (d.color[2] * 255.) as u8,
    );
    let width = (d.view_width(p) as f32 * zoom).max(0.1);
    if d.kind == Kind::FilledRectangle {
        ui.painter().add(egui::Shape::convex_polygon(
            vec![pos(0., 0.), pos(1., 0.), pos(1., 1.), pos(0., 1.)],
            color,
            Stroke::NONE,
        ));
        return;
    }
    let scale = width as f64 / d.width;
    let draw = |points: Vec<Pos2>| {
        super::drawing_style::stroke_path(ui.painter(), &points, width, color, &d.style, scale)
    };
    if d.kind == Kind::Line {
        draw(vec![pos(0., 0.), pos(1., 0.)]);
    } else if d.kind == Kind::Circle {
        draw(
            (0..=96)
                .map(|i| {
                    let t = i as f64 / 96. * std::f64::consts::TAU;
                    pos(0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin())
                })
                .collect(),
        );
    } else {
        draw(vec![
            pos(0., 0.),
            pos(1., 0.),
            pos(1., 1.),
            pos(0., 1.),
            pos(0., 0.),
        ]);
        if d.kind == Kind::Table {
            for r in 1..d.rows {
                let y = r as f64 / d.rows as f64;
                draw(vec![pos(0., y), pos(1., y)]);
            }
            for c in 1..d.columns {
                let x = c as f64 / d.columns as f64;
                draw(vec![pos(x, 0.), pos(x, 1.)]);
            }
        }
    }
}
impl App {
    fn delete_selected_drawings(&mut self) {
        if !self.permit() {
            return;
        }
        let page = self.drawing.page;
        let changes = self
            .drawing
            .selected
            .iter()
            .map(|i| (i.target, None))
            .collect::<Vec<_>>();
        let result = if self.drawing.linked_text {
            self.editor.as_mut().unwrap().delete_drawing_table(
                page,
                &changes,
                user_bounds(&self.drawing.selected),
            )
        } else {
            self.editor
                .as_mut()
                .unwrap()
                .change_drawings(page, &changes)
        };
        if self.changed(result, page) {
            self.drawing.selected.clear();
            self.drawing.linked_text = false;
            self.drawing.drag = None;
        }
    }
    fn connected_table(&mut self) -> Option<Vec<Item>> {
        let seed = self.drawing.selected.first()?.target;
        let items = self
            .drawing_items(self.drawing.page)
            .ok()?
            .into_iter()
            .filter(|i| {
                i.drawing.kind == Kind::FilledRectangle
                    || (i.drawing.kind == Kind::Line
                        && (i.drawing.u[0].abs() < 0.01 || i.drawing.u[1].abs() < 0.01))
            })
            .collect::<Vec<_>>();
        let bounds = |i: &Item| {
            i.drawing
                .view_points(&self.editor.as_ref().unwrap().info.pages[self.drawing.page])
                .iter()
                .fold(Rect::NOTHING, |r, p| {
                    r.union(Rect::from_min_size(Pos2::new(p[0], p[1]), Vec2::ZERO))
                })
        };
        let mut selected = items
            .iter()
            .filter(|i| i.target == seed)
            .cloned()
            .collect::<Vec<_>>();
        loop {
            let next = items
                .iter()
                .filter(|i| {
                    !selected.iter().any(|s| s.target == i.target)
                        && selected
                            .iter()
                            .any(|s| bounds(s).expand(1.5).intersects(bounds(i).expand(1.5)))
                })
                .cloned()
                .collect::<Vec<_>>();
            if next.is_empty() {
                break;
            }
            selected.extend(next);
        }
        let lines = selected
            .iter()
            .filter(|i| i.drawing.kind == Kind::Line)
            .collect::<Vec<_>>();
        (lines.len() >= 4
            && lines.iter().any(|i| i.drawing.u[0].abs() > 3.)
            && lines.iter().any(|i| i.drawing.u[1].abs() > 3.))
        .then_some(selected)
    }
    fn select_connected_table(&mut self) {
        if let Some(items) = self.connected_table() {
            self.drawing.selected = items;
            self.drawing.linked_text = true;
            self.drawing.mode = None;
        }
    }
    /// In Modifier, vectors use the ordinary selection tool, alongside text/images.
    pub(super) fn drawing_capture(
        &mut self,
        ui: &egui::Ui,
        page: usize,
        p: &PageInfo,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        if self
            .drawing
            .drag
            .as_ref()
            .is_some_and(|g| g.page == page && !g.committed)
        {
            return true;
        }
        let input = response.clicked() || response.drag_started_by(egui::PointerButton::Primary);
        if !input {
            if response.hovered()
                && let Some(pos) = ui.ctx().pointer_hover_pos()
            {
                let q = (pos - rect.min) / self.zoom;
                let point = Pos2::new(q.x, q.y);
                if self.hit(page, p, point).is_none()
                    && self
                        .drawing_items(page)
                        .is_ok_and(|items| closest(&items, p, point, 6. / self.zoom).is_some())
                {
                    ui.ctx().set_cursor_icon(moving_cursor(ui.ctx()));
                }
            }
            return self.drawing.page == page && !self.drawing.selected.is_empty();
        }
        let pos = if response.drag_started() {
            ui.input(|i| i.pointer.press_origin())
        } else {
            response.interact_pointer_pos()
        };
        let Some(pos) = pos else {
            return false;
        };
        let local = (pos - rect.min) / self.zoom;
        let point = Pos2::new(local.x, local.y);
        // A label inside a large frame remains a text target.
        if self.hit(page, p, point).is_none() {
            let current = self.drawing.page == page
                && self.drawing.selected.iter().any(|i| {
                    i.drawing
                        .view_points(p)
                        .iter()
                        .any(|a| point.distance(Pos2::new(a[0], a[1])) <= 9. / self.zoom)
                });
            let group = self.drawing.page == page
                && self.drawing.selected.len() > 1
                && self
                    .drawing
                    .selected
                    .iter()
                    .fold(Rect::NOTHING, |r, i| r.union(bounds(&i.drawing, p)))
                    .expand(6. / self.zoom)
                    .contains(point);
            if current
                || group
                || self
                    .drawing_items(page)
                    .is_ok_and(|items| items.iter().any(|i| hit(i, p, point, 6. / self.zoom)))
            {
                if self.finish_text() {
                    self.inline = None;
                    self.selected = None;
                    self.text_selection = None;
                    self.close_search();
                } else {
                    return false;
                }
                return true;
            }
        }
        self.drawing.selected.clear();
        self.drawing.linked_text = false;
        self.drawing.drag = None;
        false
    }
    fn drawing_items(&mut self, page: usize) -> Result<Vec<Item>> {
        let editor = self.editor.as_ref().ok_or("Aucun document.")?;
        if let Some((bytes, items)) = self.drawing.inventory.get(&page) {
            if Arc::ptr_eq(bytes, &editor.bytes) {
                return Ok(items.clone());
            }
        }
        let items = editor.drawings(page)?;
        if self.drawing.inventory.len() >= 32 {
            self.drawing.inventory.clear();
        }
        self.drawing
            .inventory
            .insert(page, (editor.bytes.clone(), items.clone()));
        Ok(items)
    }
    pub(super) fn drawing_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.drawing.drag = None;
            self.drawing.selected.clear();
            self.drawing.mode = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.drawing.linked_text = false;
            if let Some(e) = &self.editor {
                self.drawing.selected = e.drawings(self.page).unwrap_or_default();
                self.drawing.page = self.page;
                self.drawing.mode = None;
            }
        }
        if self.drawing.selected.is_empty() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            self.delete_selected_drawings();
            return;
        }
        let step = if ctx.input(|i| i.modifiers.shift) {
            10.
        } else {
            1.
        };
        for (key, delta) in [
            (Key::ArrowLeft, [-step, 0.]),
            (Key::ArrowRight, [step, 0.]),
            (Key::ArrowUp, [0., -step]),
            (Key::ArrowDown, [0., step]),
        ] {
            if ctx.input(|i| i.key_pressed(key)) {
                let p = &self.editor.as_ref().unwrap().info.pages[self.drawing.page];
                let a = p.view_to_user(0., 0.);
                let b = p.view_to_user(delta[0], delta[1]);
                let items = self
                    .drawing
                    .selected
                    .iter()
                    .map(|i| {
                        let mut i = i.clone();
                        i.drawing
                            .translate([(b[0] - a[0]) as f64, (b[1] - a[1]) as f64]);
                        i
                    })
                    .collect();
                self.commit_drawings(items);
            }
        }
    }
    pub(super) fn drawing_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Dessin");
            if icons::small(ui, Icon::Close, "")
                .on_hover_text("Fermer le dessin")
                .clicked()
            {
                self.set_tool(ui.ctx(), Tool::Select);
            }
        });
        for pair in [
            (None, Icon::Select, "Sélectionner un tracé", false),
            (Some(Kind::Line), Icon::Line, "Trait", false),
            (Some(Kind::Circle), Icon::Circle, "Cercle", true),
            (Some(Kind::Rectangle), Icon::Square, "Carré", true),
            (Some(Kind::Rectangle), Icon::Rectangle, "Rectangle", false),
            (Some(Kind::Table), Icon::Table, "Tableau", false),
        ] {
            if icons::chip(
                ui,
                pair.1,
                pair.2,
                self.drawing.mode == pair.0 && self.drawing.square == pair.3,
            )
            .clicked()
            {
                if self.tool != Tool::Draw {
                    self.set_tool(ui.ctx(), Tool::Draw);
                }
                self.drawing.mode = pair.0;
                self.drawing.square = pair.3;
                if pair.0 != Some(Kind::Line) && self.drawing.style == StrokeStyle::Double {
                    self.drawing.style = StrokeStyle::Solid;
                }
                self.drawing.selected.clear();
                self.drawing.all = false;
            }
        }
        ui.add_space(10.);
        if icons::small(ui, Icon::Select, "Tout sélectionner").clicked() {
            if let Some(e) = &self.editor {
                match e.drawings(self.page) {
                    Ok(items) => {
                        self.drawing.selected = items;
                        self.drawing.page = self.page;
                        self.drawing.mode = None;
                        self.drawing.all = true;
                        self.drawing.linked_text = false;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        }
        if !self.drawing.selected.is_empty()
            && !self.drawing.linked_text
            && self.connected_table().is_some()
            && ui.button("Sélectionner le tableau").clicked()
        {
            self.select_connected_table();
        }
        let Some(p) = self
            .editor
            .as_ref()
            .and_then(|e| {
                e.info.pages.get(if self.drawing.selected.is_empty() {
                    self.page
                } else {
                    self.drawing.page
                })
            })
            .cloned()
        else {
            return;
        };
        let mut selected = self.drawing.selected.clone();
        if let Some(table) = selected.first().filter(|i| i.drawing.kind == Kind::Table) {
            self.drawing.rows = table.drawing.rows;
            self.drawing.columns = table.drawing.columns;
        }
        let mut width = selected
            .first()
            .map_or(self.drawing.width, |i| i.drawing.view_width(&p));
        let mut color = selected
            .first()
            .map_or(self.drawing.color, |i| i.drawing.color.map(|c| c as f32));
        let mut style = selected
            .first()
            .map_or_else(|| self.drawing.style.clone(), |i| i.drawing.style.clone());
        let allow_double = if selected.is_empty() {
            self.drawing.mode == Some(Kind::Line) || self.drawing.mode.is_none()
        } else {
            selected.iter().all(|i| i.drawing.kind == Kind::Line)
        };
        ui.separator();
        let filled = !selected.is_empty()
            && selected
                .iter()
                .all(|i| i.drawing.kind == Kind::FilledRectangle);
        ui.label(if filled {
            "Couleur du fond"
        } else {
            "Couleur du trait"
        });
        let color_changed = self.palette(ui, &mut color);
        ui.add_space(6.);
        if !filled {
            ui.label("Épaisseur");
        }
        let width_changed = if filled {
            false
        } else {
            super::drawing_style::width_picker(ui, &mut width)
        };
        ui.add_space(6.);
        if !filled {
            ui.label("Type de trait");
        }
        let style_changed = if filled {
            false
        } else {
            super::drawing_style::style_picker(ui, &mut style, allow_double)
        };
        if selected.is_empty() {
            self.drawing.width = width;
            self.drawing.color = color;
            self.drawing.style = style.clone();
        }
        let mut changed = color_changed || width_changed || style_changed;
        let mut table_changed = false;
        if self.drawing.mode == Some(Kind::Table)
            || selected
                .first()
                .is_some_and(|i| i.drawing.kind == Kind::Table)
        {
            ui.horizontal(|ui| {
                ui.label("Lignes");
                table_changed |= ui
                    .add(egui::DragValue::new(&mut self.drawing.rows).range(1..=100))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Colonnes");
                table_changed |= ui
                    .add(egui::DragValue::new(&mut self.drawing.columns).range(1..=100))
                    .changed();
            });
        }
        changed |= table_changed;
        if selected.len() == 1 && selected[0].drawing.kind == Kind::Line {
            let points = selected[0].drawing.view_points(&p);
            let mut length =
                (points[1][0] - points[0][0]).hypot(points[1][1] - points[0][1]) as f64;
            ui.label("Longueur");
            if ui
                .add(
                    egui::DragValue::new(&mut length)
                        .range(1.0..=10000.)
                        .suffix(" pt"),
                )
                .changed()
            {
                let old = selected[0].drawing.u[0].hypot(selected[0].drawing.u[1]);
                let unit = Drawing::from_view(
                    &p,
                    Kind::Line,
                    [0., 0.],
                    [length as f32, 0.],
                    1.,
                    [0.; 3],
                    1,
                    1,
                )
                .u;
                let desired = unit[0].hypot(unit[1]);
                if old > 0. {
                    selected[0].drawing.u = selected[0].drawing.u.map(|v| v * desired / old);
                }
                changed = true;
            }
        }
        if !selected.is_empty() {
            if selected.len() == 1
                && matches!(selected[0].drawing.kind, Kind::Table | Kind::Rectangle)
                && ui.button("Modifier les traits séparément").clicked()
                && self.permit()
            {
                let page = self.drawing.page;
                match self
                    .editor
                    .as_mut()
                    .unwrap()
                    .split_drawing(page, selected[0].target)
                {
                    Ok(targets) => {
                        self.refresh(page);
                        self.drawing.selected = self
                            .editor
                            .as_ref()
                            .unwrap()
                            .drawings(page)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|i| targets.contains(&i.target))
                            .collect();
                        self.drawing.mode = None;
                    }
                    Err(e) => self.message = Some(e),
                }
                return;
            }
            ui.label(format!("{} élément(s) sélectionné(s)", selected.len()));
            for pair in [
                [
                    (Icon::Previous, "Gauche", -5., 0.),
                    (Icon::Next, "Droite", 5., 0.),
                ],
                [(Icon::Up, "Haut", 0., -5.), (Icon::Down, "Bas", 0., 5.)],
            ] {
                ui.horizontal(|ui| {
                    for (icon, label, dx, dy) in pair {
                        if icons::small(ui, icon, label).clicked() {
                            let a = p.view_to_user(0., 0.);
                            let b = p.view_to_user(dx, dy);
                            for item in &mut selected {
                                item.drawing
                                    .translate([(b[0] - a[0]) as f64, (b[1] - a[1]) as f64]);
                            }
                            changed = true;
                        }
                    }
                });
            }
            if ui
                .button(if self.drawing.linked_text {
                    "Supprimer le tableau"
                } else {
                    "Supprimer la sélection"
                })
                .clicked()
            {
                self.delete_selected_drawings();
                return;
            }
        }
        if changed && !selected.is_empty() {
            for item in &mut selected {
                if width_changed {
                    item.drawing.width *= width / item.drawing.view_width(&p).max(0.01);
                }
                if color_changed {
                    item.drawing.color = color.map(f64::from);
                }
                if style_changed {
                    item.drawing.style = style.clone();
                }
                if table_changed && item.drawing.kind == Kind::Table {
                    item.drawing.rows = self.drawing.rows;
                    item.drawing.columns = self.drawing.columns;
                }
            }
            self.commit_drawings(selected);
        }
    }
    fn commit_drawings(&mut self, items: Vec<Item>) {
        if !self.permit() {
            return;
        }
        let page = self.drawing.page;
        let changes = items
            .iter()
            .map(|i| (i.target, Some(i.drawing.clone())))
            .collect::<Vec<_>>();
        let delta = items
            .first()
            .zip(self.drawing.selected.first())
            .map(|(a, b)| {
                [
                    a.drawing.a[0] - b.drawing.a[0],
                    a.drawing.a[1] - b.drawing.a[1],
                ]
            });
        let translation = delta.filter(|delta| {
            items.iter().zip(&self.drawing.selected).all(|(a, b)| {
                let mut old = b.drawing.clone();
                old.translate(*delta);
                old == a.drawing
            })
        });
        let result = if self.drawing.linked_text
            && let Some(delta) = translation
            && delta != [0., 0.]
        {
            let r = user_bounds(&self.drawing.selected);
            self.editor
                .as_mut()
                .unwrap()
                .move_drawing_table(page, &changes, r, delta)
        } else {
            self.editor
                .as_mut()
                .unwrap()
                .change_drawings(page, &changes)
        };
        if self.changed(result, page) {
            let current = self
                .editor
                .as_ref()
                .unwrap()
                .drawings(page)
                .unwrap_or_default();
            self.drawing.selected = current
                .into_iter()
                .filter(|i| {
                    items.iter().any(|old| {
                        (matches!(old.target, DrawingTarget::Owned(_)) && i.target == old.target)
                            || (i.drawing.kind == old.drawing.kind
                                && i.drawing
                                    .a
                                    .iter()
                                    .chain(&i.drawing.u)
                                    .chain(&i.drawing.v)
                                    .zip(
                                        old.drawing
                                            .a
                                            .iter()
                                            .chain(&old.drawing.u)
                                            .chain(&old.drawing.v),
                                    )
                                    .all(|(a, b)| (a - b).abs() < 0.01))
                    })
                })
                .collect();
        }
    }
    pub(super) fn drawing_canvas(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        p: &PageInfo,
        rect: Rect,
        response: &egui::Response,
    ) {
        let items = match self.drawing_items(page) {
            Ok(i) => i,
            Err(e) => {
                if response.clicked() {
                    self.message = Some(e);
                }
                return;
            }
        };
        let zoom = self.zoom;
        let local = |pos: Pos2| {
            let q = (pos - rect.min) / zoom;
            Pos2::new(q.x, q.y)
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(if self.drawing.mode.is_some() {
                egui::CursorIcon::Crosshair
            } else {
                let at = ui.ctx().pointer_hover_pos().map(local);
                if at.is_some_and(|point| {
                    self.drawing.selected.iter().any(|i| {
                        i.drawing
                            .view_points(p)
                            .iter()
                            .any(|a| point.distance(Pos2::new(a[0], a[1])) < 9. / self.zoom)
                    })
                }) {
                    egui::CursorIcon::Crosshair
                } else if at
                    .is_some_and(|point| closest(&items, p, point, 6. / self.zoom).is_some())
                {
                    moving_cursor(ui.ctx())
                } else {
                    egui::CursorIcon::Default
                }
            });
        }
        if response.clicked() && self.drawing.drag.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                let pos = local(pos);
                self.drawing.page = page;
                if let Some(kind) = self.drawing.mode {
                    let size = if kind == Kind::Line {
                        Vec2::new(120., 0.)
                    } else {
                        Vec2::new(150., if self.drawing.square { 150. } else { 90. })
                    };
                    let d = Drawing::from_view(
                        p,
                        kind,
                        [pos.x, pos.y],
                        [pos.x + size.x, pos.y + size.y],
                        self.drawing.width,
                        self.drawing.color.map(f64::from),
                        self.drawing.rows,
                        self.drawing.columns,
                    )
                    .with_style(self.drawing.style.clone());
                    if self.permit() {
                        match self.editor.as_mut().unwrap().add_drawing(page, d.clone()) {
                            Ok(r) => {
                                self.refresh(page);
                                self.drawing.selected = vec![Item {
                                    target: DrawingTarget::Owned(r),
                                    drawing: d,
                                }];
                                self.drawing.mode = None;
                            }
                            Err(e) => self.message = Some(e),
                        }
                    }
                } else {
                    self.drawing.linked_text = false;
                    self.drawing.selected = closest(&items, p, pos, 6. / self.zoom)
                        .into_iter()
                        .collect();
                }
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                let start = local(start);
                self.drawing.page = page;
                let (original, draft, action) = if let Some(kind) = self.drawing.mode {
                    (
                        Vec::new(),
                        vec![
                            Drawing::from_view(
                                p,
                                kind,
                                [start.x, start.y],
                                [start.x + 1., start.y + 1.],
                                self.drawing.width,
                                self.drawing.color.map(f64::from),
                                self.drawing.rows,
                                self.drawing.columns,
                            )
                            .with_style(self.drawing.style.clone()),
                        ],
                        Action::New,
                    )
                } else {
                    let selected_bounds = self
                        .drawing
                        .selected
                        .iter()
                        .fold(Rect::NOTHING, |r, i| r.union(bounds(&i.drawing, p)));
                    let corner = if self.drawing.selected.len() == 1 {
                        let d = &self.drawing.selected[0].drawing;
                        let points = if d.kind == Kind::Line {
                            d.view_points(p)
                                .into_iter()
                                .map(|v| Pos2::new(v[0], v[1]))
                                .collect::<Vec<_>>()
                        } else {
                            vec![selected_bounds.left_top(), selected_bounds.right_bottom()]
                        };
                        points
                            .iter()
                            .position(|q| q.distance(start) < 9. / self.zoom)
                    } else {
                        None
                    };
                    let selected = if corner.is_some()
                        || (self.drawing.selected.len() > 1
                            && selected_bounds.expand(6. / self.zoom).contains(start))
                    {
                        self.drawing.selected.clone()
                    } else {
                        closest(&items, p, start, 7. / self.zoom)
                            .into_iter()
                            .collect()
                    };
                    let draft = selected.iter().map(|i| i.drawing.clone()).collect();
                    (selected, draft, corner.map_or(Action::Move, Action::Corner))
                };
                if !draft.is_empty() && self.permit() {
                    let background = if original.is_empty() {
                        None
                    } else {
                        match self.editor.as_ref().unwrap().preview_without_drawings(
                            page,
                            &original.iter().map(|i| i.target).collect::<Vec<_>>(),
                        ) {
                            Ok(bytes) => Some(raster::Background::new(
                                bytes,
                                page,
                                (self.zoom * ui.ctx().pixels_per_point()).clamp(0.25, 4.),
                            )),
                            Err(e) => {
                                self.message = Some(e);
                                return;
                            }
                        }
                    };
                    self.drawing.selected = original.clone();
                    self.drawing.drag = Some(Gesture {
                        page,
                        panel_visible: self.right_rect.is_positive(),
                        start,
                        original,
                        draft,
                        kind: action,
                        background,
                        committed: false,
                    });
                }
            }
        }
        if let Some(g) = &mut self.drawing.drag {
            if g.page == page && !g.committed {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    let mut end = {
                        let q = (pos - rect.min) / self.zoom;
                        Pos2::new(q.x, q.y)
                    };
                    match g.kind {
                        Action::New => {
                            if self.drawing.square {
                                let d = end - g.start;
                                let n = d.x.abs().max(d.y.abs());
                                end = g.start + Vec2::new(n * d.x.signum(), n * d.y.signum());
                            }
                            let old = &g.draft[0];
                            g.draft[0] = Drawing::from_view(
                                p,
                                old.kind,
                                [g.start.x, g.start.y],
                                [end.x, end.y],
                                self.drawing.width,
                                self.drawing.color.map(f64::from),
                                self.drawing.rows,
                                self.drawing.columns,
                            )
                            .with_style(self.drawing.style.clone());
                        }
                        Action::Move => {
                            let a = p.view_to_user(g.start.x, g.start.y);
                            let b = p.view_to_user(end.x, end.y);
                            g.draft = g
                                .original
                                .iter()
                                .map(|i| {
                                    let mut d = i.drawing.clone();
                                    d.translate([(b[0] - a[0]) as f64, (b[1] - a[1]) as f64]);
                                    d
                                })
                                .collect();
                        }
                        Action::Corner(c) => {
                            let old = &g.original[0].drawing;
                            let original = bounds(old, p);
                            let points = old.view_points(p);
                            let original_a = if old.kind == Kind::Line {
                                Pos2::new(points[0][0], points[0][1])
                            } else {
                                original.left_top()
                            };
                            let original_b = if old.kind == Kind::Line {
                                Pos2::new(points[1][0], points[1][1])
                            } else {
                                original.right_bottom()
                            };
                            let (mut a, mut b) = if c == 0 {
                                (end, original_b)
                            } else {
                                (original_a, end)
                            };
                            let square = (original.width() - original.height()).abs() < 0.01;
                            if old.kind == Kind::Circle || (old.kind == Kind::Rectangle && square) {
                                let delta = b - a;
                                let side = delta.x.abs().max(delta.y.abs());
                                let delta =
                                    Vec2::new(side * delta.x.signum(), side * delta.y.signum());
                                if c == 0 {
                                    a = b - delta;
                                } else {
                                    b = a + delta;
                                }
                            }
                            g.draft[0] = Drawing::from_view(
                                p,
                                old.kind,
                                [a.x, a.y],
                                [b.x, b.y],
                                old.view_width(p),
                                old.color,
                                old.rows,
                                old.columns,
                            )
                            .with_style(old.style.clone());
                        }
                    }
                }
            }
        }
        let release = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        if release
            && self
                .drawing
                .drag
                .as_ref()
                .is_some_and(|g| g.page == page && !g.committed)
        {
            let mut g = self.drawing.drag.take().unwrap();
            if matches!(g.kind, Action::New) {
                match self
                    .editor
                    .as_mut()
                    .unwrap()
                    .add_drawing(page, g.draft[0].clone())
                {
                    Ok(r) => {
                        self.refresh(page);
                        self.drawing.selected = vec![Item {
                            target: DrawingTarget::Owned(r),
                            drawing: g.draft[0].clone(),
                        }];
                        self.drawing.mode = None;
                        g.committed = true;
                    }
                    Err(e) => self.message = Some(e),
                }
            } else {
                let changed = self.editor.as_ref().unwrap().revision();
                self.commit_drawings(
                    g.original
                        .iter()
                        .zip(&g.draft)
                        .map(|(i, d)| Item {
                            target: i.target,
                            drawing: d.clone(),
                        })
                        .collect(),
                );
                g.committed = self.editor.as_ref().unwrap().revision() != changed;
            }
            if g.committed {
                self.drawing.drag = Some(g);
            }
        }
        if let Some(g) = &self.drawing.drag {
            if g.page == page {
                for d in &g.draft {
                    paint(ui, d, p, rect, self.zoom);
                }
            }
        }
        if self.drawing.page == page {
            let drawings = self
                .drawing
                .drag
                .as_ref()
                .filter(|g| g.page == page)
                .map(|g| g.draft.clone())
                .unwrap_or_else(|| {
                    self.drawing
                        .selected
                        .iter()
                        .map(|i| i.drawing.clone())
                        .collect()
                });
            let r = drawings
                .iter()
                .fold(Rect::NOTHING, |r, d| r.union(bounds(d, p)));
            if r.is_finite() {
                let r = Rect::from_min_max(
                    rect.min + r.min.to_vec2() * self.zoom,
                    rect.min + r.max.to_vec2() * self.zoom,
                )
                .expand(3.);
                ui.painter()
                    .rect_stroke(r, 2., Stroke::new(1., icons::BLUE), StrokeKind::Outside);
                let handles = if drawings.len() == 1 && drawings[0].kind == Kind::Line {
                    drawings[0]
                        .view_points(p)
                        .into_iter()
                        .map(|a| rect.min + Vec2::new(a[0], a[1]) * self.zoom)
                        .collect::<Vec<_>>()
                } else {
                    vec![r.left_top(), r.right_bottom()]
                };
                for point in handles {
                    ui.painter().circle_filled(point, 4., Color32::WHITE);
                    ui.painter()
                        .circle_stroke(point, 4., Stroke::new(1., icons::BLUE));
                }
            }
        }
    }
}
