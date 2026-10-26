use super::*;
#[cfg(all(windows, not(test)))]
fn clipboard_sequence() -> u32 {
    clipboard_win::seq_num().map_or(0, |n| n.get())
}
#[cfg(any(not(windows), test))]
fn clipboard_sequence() -> u32 {
    0
}
impl App {
    pub(super) fn refresh_image_clipboard(&mut self, ctx: &egui::Context) {
        // Platform output is applied after the frame, not between layout passes.
        if ctx.cumulative_frame_nr() <= self.copied_image_frame {
            return;
        }
        if self.copied_image.is_none() {
            return;
        }
        let sequence = clipboard_sequence();
        if self
            .copied_image_sequence
            .is_some_and(|old| old != sequence)
        {
            self.copied_image = None;
            self.copied_image_sequence = None;
        } else {
            self.copied_image_sequence = Some(sequence);
        }
    }

    pub(super) fn copy_selected_block(&mut self, ctx: &egui::Context) -> bool {
        if self.workspace != Workspace::Edit || !self.finish_text() {
            return false;
        }
        if let Some(s) = self.selected.clone().filter(|s| s.is_image) {
            let target = match s.target {
                Target::Image(i) => pdfcraft_edit::ObjectTarget {
                    kind: ObjectKind::Image,
                    index: i,
                },
                Target::Added(i) => pdfcraft_edit::ObjectTarget {
                    kind: ObjectKind::Added,
                    index: i,
                },
                _ => return false,
            };
            let result = self
                .editor
                .as_ref()
                .ok_or("Document absent.".to_string())
                .and_then(|e| e.copy_image_block(s.page, target));
            return match result {
                Ok(copied) => {
                    if let Ok(preview) = copied.clipboard_preview() {
                        ctx.copy_image(egui::ColorImage::from_rgba_premultiplied(
                            [preview.width as usize, preview.height as usize],
                            &preview.rgba,
                        ));
                    }
                    self.copied_image = Some(copied);
                    self.copied_image_sequence = None;
                    self.copied_image_frame = ctx.cumulative_frame_nr();
                    self.copied_block = None;
                    self.message = None;
                    true
                }
                Err(error) => {
                    self.message = Some(error);
                    false
                }
            };
        }
        let Some(s) = self
            .selected
            .clone()
            .filter(|s| !s.is_image && matches!(s.target, Target::Block(_) | Target::Added(_)))
        else {
            return false;
        };
        let Some(e) = &self.editor else {
            return false;
        };
        let result = e.copy_text_block(
            s.page,
            if let Target::Block(i) = s.target {
                Some(i)
            } else {
                None
            },
            if let Target::Added(i) = s.target {
                Some(i)
            } else {
                None
            },
        );
        match result {
            Ok(copied) => {
                ctx.copy_text(copied.data.text.clone());
                self.copied_block = Some(copied);
                self.copied_image = None;
                self.message = None;
                true
            }
            Err(error) => {
                self.message = Some(error);
                false
            }
        }
    }
    pub(super) fn cut_selected_block(&mut self, ctx: &egui::Context) {
        if self.copy_selected_block(ctx) {
            self.delete();
        }
    }
    /// Frame controls own their context menu; text inside keeps its text editor menu.
    pub(super) fn block_frame_menu(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        s: &Selection,
    ) {
        if self.workspace != Workspace::Edit {
            return;
        }
        if response.clicked_by(egui::PointerButton::Primary) || response.secondary_clicked() {
            if !self.finish_text() {
                return;
            }
            self.page = s.page;
            self.selected = self
                .selection_for(s.page, s.target)
                .or_else(|| Some(s.clone()));
            ui.ctx()
                .memory_mut(|m| m.surrender_focus(egui::Id::new("direct-text")));
        }
        response.context_menu(|ui| self.block_menu_contents(ui, s));
    }
    fn block_menu_contents(&mut self, ui: &mut egui::Ui, s: &Selection) {
        let can_copy = matches!(
            s.target,
            Target::Block(_) | Target::Added(_) | Target::Image(_)
        ) && self.editor.as_ref().is_some_and(Editor::can_copy);
        if ui
            .add_enabled(
                can_copy,
                egui::Button::new("Copier").shortcut_text("Ctrl+C"),
            )
            .clicked()
        {
            self.copy_selected_block(ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(
                can_copy,
                egui::Button::new("Couper").shortcut_text("Ctrl+X"),
            )
            .clicked()
        {
            self.cut_selected_block(ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(
                self.copied_block.is_some() || self.copied_image.is_some(),
                egui::Button::new("Coller").shortcut_text("Ctrl+V"),
            )
            .clicked()
        {
            self.paste_copied_block();
            ui.close();
        }
        if s.is_image && ui.button("Rogner").clicked() {
            self.begin_image_crop(ui.ctx(), s.clone());
            ui.close();
        }
        ui.separator();
        if ui
            .add(egui::Button::new("Supprimer").shortcut_text("Suppr"))
            .clicked()
        {
            self.delete();
            ui.close();
        }
    }
    pub(super) fn paste_copied_block(&mut self) {
        if self.workspace != Workspace::Edit || !self.finish_text() || !self.permit() {
            return;
        }
        if let Some(copied) = self.copied_image.clone() {
            let Some(e) = &mut self.editor else { return };
            let Some(p) = e.info.pages.get(self.page) else {
                return;
            };
            let w = copied.view_rect[2] - copied.view_rect[0];
            let h = copied.view_rect[3] - copied.view_rect[1];
            let at = [
                (copied.view_rect[0] + 16.).clamp(0., (p.width - w).max(0.)),
                (copied.view_rect[1] + 16.).clamp(0., (p.height - h).max(0.)),
            ];
            match e.paste_image_block(self.page, &copied, at) {
                Ok(index) => {
                    self.changed(Ok(()), self.page);
                    self.image_crop = None;
                    self.image_context = None;
                    self.text_selection = None;
                    self.close_search();
                    self.jump = Some(self.page);
                    self.marquee.clear();
                    self.inline = None;
                    self.selected = self.selection_for(self.page, Target::Image(index));
                    self.tool = Tool::Move;
                }
                Err(error) => self.message = Some(error),
            }
            return;
        }
        let Some(copied) = self.copied_block.clone() else {
            return;
        };
        let Some(e) = &mut self.editor else {
            return;
        };
        let Some(p) = e.info.pages.get(self.page) else {
            return;
        };
        let width = copied.view_rect[2] - copied.view_rect[0];
        let height = copied.view_rect[3] - copied.view_rect[1];
        // Keep the original position where it fits; offset a duplicate slightly.
        let at = [
            (copied.view_rect[0] + 16.).clamp(8., (p.width - width - 8.).max(8.)),
            (copied.view_rect[1] + 16.).clamp(8., (p.height - height - 8.).max(8.)),
        ];
        let result = e.paste_text_block(self.page, &copied, at);
        match result {
            Ok(pasted) => {
                self.changed(Ok(()), self.page);
                self.inline = None;
                self.drag = None;
                self.selected = self.selection_for(
                    self.page,
                    match pasted {
                        pdfinator::transfer::PastedText::Block(index) => Target::Block(index),
                        pdfinator::transfer::PastedText::Added(index) => Target::Added(index),
                    },
                );
                self.text_selection = None;
                self.close_search();
                self.jump = Some(self.page);
                self.tool = Tool::Edit;
            }
            Err(error) => self.message = Some(error),
        }
    }
    pub(super) fn block_clipboard_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.workspace != Workspace::Edit || self.search {
            return false;
        }
        let inline = self.inline.as_ref().filter(|i| !i.committed);
        // An actual character selection retains the normal text clipboard.
        let copy_block = self.selected.as_ref().is_some_and(|s| {
            matches!(
                s.target,
                Target::Block(_) | Target::Added(_) | Target::Image(_)
            )
        }) && self.text_selection.is_none()
            && inline.is_none_or(|i| i.selection.text_range.as_ref().is_none_or(|r| r.is_empty()))
            && (inline.is_some() || !ctx.egui_wants_keyboard_input());
        if copy_block
            && ctx.input_mut(|i| {
                let key = i.consume_key(Modifiers::CTRL, Key::X);
                let event = i.events.iter().any(|e| matches!(e, egui::Event::Cut));
                if key || event {
                    i.events.retain(|e| !matches!(e, egui::Event::Cut));
                }
                key || event
            })
        {
            self.cut_selected_block(ctx);
            return true;
        }
        if copy_block
            && ctx.input_mut(|i| {
                let key = i.consume_key(Modifiers::CTRL, Key::C);
                let event = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
                if key || event {
                    i.events.retain(|e| !matches!(e, egui::Event::Copy));
                }
                key || event
            })
        {
            self.copy_selected_block(ctx);
            return true;
        }
        if inline.is_none()
            && !ctx.egui_wants_keyboard_input()
            && self.copied_image.is_some()
            // egui-winit consumes the press of Ctrl+V while checking the OS
            // text clipboard. For an image it only forwards the release.
            && ctx.input_mut(|i| {
                let paste=i.events.iter().any(|event|matches!(event,egui::Event::Key{key:Key::V,pressed:false,modifiers,..} if modifiers.ctrl));
                if paste {i.events.retain(|event|!matches!(event,egui::Event::Key{key:Key::V,pressed:false,modifiers,..} if modifiers.ctrl));}
                paste
            })
        {
            self.paste_copied_block();
            return true;
        }
        if inline.is_none() && !ctx.egui_wants_keyboard_input() && self.copied_block.is_some() {
            let text = &self.copied_block.as_ref().unwrap().data.text;
            let paste = ctx.input_mut(|i| {
                let event = i.events.iter().any(|e| matches!(e, egui::Event::Paste(s) if s.replace("\r\n", "\n") == text.replace("\r\n", "\n")));
                let has_text = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
                let key = i.consume_key(Modifiers::CTRL, Key::V);
                let paste = event || (key && !has_text);
                if paste { i.events.retain(|e| !matches!(e, egui::Event::Paste(_))); }
                paste
            });
            if paste {
                self.paste_copied_block();
                return true;
            }
        }
        false
    }
    pub(super) fn image_page_menu(
        &mut self,
        ui: &egui::Ui,
        page: usize,
        p: &PageInfo,
        sheet: Rect,
        response: &egui::Response,
    ) {
        if self.workspace != Workspace::Edit {
            return;
        }
        if response.secondary_clicked() {
            self.image_context = None;
            if let Some(pos) = response.interact_pointer_pos() {
                let q = (pos - sheet.min) / self.zoom;
                if let Some(s) = self
                    .hit(page, p, Pos2::new(q.x, q.y))
                    .filter(|s| s.is_image)
                {
                    if !self.finish_text() {
                        return;
                    }
                    self.inline = None;
                    self.close_search();
                    ui.ctx()
                        .memory_mut(|m| m.surrender_focus(egui::Id::new("direct-text")));
                    self.marquee.clear();
                    self.drawing.selected.clear();
                    self.text_selection = None;
                    self.page = page;
                    self.selected = Some(s.clone());
                    self.image_context = Some(s);
                }
            }
        }
        if let Some(s) = self.image_context.clone().filter(|s| s.page == page) {
            egui::Popup::context_menu(response)
                .id(egui::Id::new((
                    "image-page-menu",
                    self.active_document,
                    page,
                )))
                .show(|ui| self.block_menu_contents(ui, &s));
        }
    }
}
