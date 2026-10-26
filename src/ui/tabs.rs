use super::*;
impl App {
    fn title_drag(&self, ctx: &egui::Context, response: egui::Response) {
        if response.double_clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
            ));
        } else if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
    pub(super) fn move_document_tab(&mut self, from: usize, to: usize) {
        if from >= self.documents.len() || to >= self.documents.len() || from == to {
            return;
        }
        let state = self.documents.remove(from);
        self.documents.insert(to, state);
        self.active_document = if self.active_document == from {
            to
        } else if from < self.active_document && self.active_document <= to {
            self.active_document - 1
        } else if to <= self.active_document && self.active_document < from {
            self.active_document + 1
        } else {
            self.active_document
        };
    }
    pub(super) fn tabs(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        if self.tab_drag.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.tab_drag = None;
        }
        egui::Panel::top("workspace-tabs")
            .exact_size(40.)
            .frame(egui::Frame::new().fill(icons::RAIL).inner_margin(4.))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let (logo, response) =
                        ui.allocate_exact_size(Vec2::new(25., 28.), egui::Sense::click_and_drag());
                    icons::draw(ui.painter(), Icon::Brand, logo.shrink(1.), Color32::WHITE);
                    self.title_drag(&ctx, response);
                    for (tab, label) in [
                        (Workspace::Read, "Lecture"),
                        (Workspace::Edit, "Modifier"),
                        (Workspace::Pages, "Pages"),
                    ] {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(label).color(Color32::WHITE).size(12.),
                                )
                                .fill(if self.workspace == tab {
                                    Color32::from_rgb(243, 41, 94)
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .stroke(Stroke::NONE)
                                .corner_radius(6.)
                                .min_size(Vec2::new(65., 29.)),
                            )
                            .clicked()
                        {
                            self.set_workspace(&ctx, tab);
                        }
                    }
                    ui.add_space(8.);
                    let help = ui
                        .add(
                            egui::Button::new("?")
                                .frame(false)
                                .min_size(Vec2::new(25., 28.)),
                        )
                        .on_hover_text("Aide et raccourcis · F1");
                    help.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            "Aide et raccourcis · F1",
                        )
                    });
                    if help.clicked() {
                        self.work.help_open = true;
                    }
                    let mut activate = None;
                    let mut close = None;
                    let mut cards = Vec::new();
                    let available = (ui.available_width() - 144.).max(32.);
                    let strip = egui::ScrollArea::horizontal()
                        .id_salt("document-tabs")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .max_width(available)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for index in 0..self.documents.len() {
                                    let label = self.document_label(index);
                                    let dirty = self.document_dirty(index);
                                    let frame = egui::Frame::new()
                                        .fill(if index == self.active_document {
                                            Color32::from_rgb(42, 61, 89)
                                        } else {
                                            Color32::from_rgb(25, 37, 57)
                                        })
                                        .corner_radius(6.)
                                        .inner_margin(3.)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let r = ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new(&label)
                                                                .color(Color32::WHITE)
                                                                .size(11.),
                                                        )
                                                        .frame(false)
                                                        .sense(egui::Sense::click_and_drag())
                                                        .min_size(Vec2::new(124., 25.)),
                                                    )
                                                    .on_hover_text(&label);
                                                if r.drag_started() {
                                                    self.tab_drag = Some(index);
                                                }
                                                if r.clicked() {
                                                    activate = Some(index);
                                                }
                                                let (dot, _) = ui.allocate_exact_size(
                                                    Vec2::new(6., 25.),
                                                    egui::Sense::hover(),
                                                );
                                                if dirty {
                                                    ui.painter().circle_filled(
                                                        dot.center(),
                                                        2.5,
                                                        Color32::from_rgb(245, 65, 80),
                                                    );
                                                }
                                                if icons::small(ui, Icon::Close, "")
                                                    .on_hover_text(format!("Fermer {label}"))
                                                    .clicked()
                                                {
                                                    close = Some(index);
                                                }
                                            });
                                        });
                                    cards.push((index, frame.response.rect));
                                }
                            });
                            if self.tab_drag.is_some() {
                                if let Some(pos) = ui.ctx().pointer_hover_pos() {
                                    let bounds = ui.clip_rect();
                                    let delta = if pos.x < bounds.left() + 26. {
                                        1.
                                    } else if pos.x > bounds.right() - 26. {
                                        -1.
                                    } else {
                                        0.
                                    };
                                    ui.scroll_with_delta(Vec2::new(
                                        delta * ui.input(|i| i.stable_dt.min(0.05)) * 420.,
                                        0.,
                                    ));
                                }
                            }
                        });
                    let add = icons::small(ui, Icon::Plus, "");
                    add.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            "Ouvrir un autre PDF",
                        )
                    });
                    if add.on_hover_text("Ouvrir un autre PDF · Ctrl+O").clicked() {
                        self.pick_open();
                    }
                    if let Some(source) = self.tab_drag {
                        if let Some(pos) = ctx.pointer_hover_pos() {
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            let target = pages::PageDropTarget::closest(source, pos, &cards, false);
                            if let Some(target) = target {
                                ui.painter().with_clip_rect(strip.inner_rect).line_segment(
                                    target.marker,
                                    Stroke::new(3., Color32::from_rgb(116, 207, 255)),
                                );
                                egui::Area::new(egui::Id::new("dragged-document-tab"))
                                    .order(egui::Order::Tooltip)
                                    .interactable(false)
                                    .fixed_pos(pos + Vec2::new(8., 14.))
                                    .show(&ctx, |ui| {
                                        egui::Frame::new()
                                            .fill(Color32::from_rgb(35, 62, 99))
                                            .corner_radius(7.)
                                            .inner_margin(9.)
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new(
                                                        self.document_label(source),
                                                    )
                                                    .color(Color32::WHITE),
                                                );
                                            });
                                    });
                                if ctx.input(|i| {
                                    i.pointer.button_released(egui::PointerButton::Primary)
                                }) {
                                    if strip.inner_rect.expand(8.).contains(pos) {
                                        self.move_document_tab(source, target.destination);
                                    }
                                    self.tab_drag = None;
                                    activate = None;
                                    close = None;
                                }
                            }
                        }
                        if !ctx.input(|i| i.pointer.primary_down()) {
                            self.tab_drag = None;
                        }
                    }
                    if let Some(index) = activate {
                        self.activate_document(index);
                    }
                    if let Some(index) = close {
                        self.close_document(index);
                    }
                    let free = ui.available_width() - 100.;
                    if free > 1. {
                        let (_, response) = ui.allocate_exact_size(
                            Vec2::new(free, 28.),
                            egui::Sense::click_and_drag(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Barre de titre",
                            )
                        });
                        self.title_drag(&ctx, response);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (icon, label) in [
                            (Icon::Close, "Fermer la fenêtre"),
                            (Icon::Maximize, "Agrandir la fenêtre"),
                            (Icon::Minimize, "Réduire la fenêtre"),
                        ] {
                            if icons::small(ui, icon, "").on_hover_text(label).clicked() {
                                ctx.send_viewport_cmd(match icon {
                                    Icon::Close => egui::ViewportCommand::Close,
                                    Icon::Minimize => egui::ViewportCommand::Minimized(true),
                                    _ => egui::ViewportCommand::Maximized(
                                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                                    ),
                                });
                            }
                        }
                    });
                });
            });
    }
}
