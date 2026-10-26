use super::*;
impl App {
    pub(super) fn comment_changed(&self) -> bool {
        let Some(s) = &self.selected else {
            return false;
        };
        let Target::Comment(index) = s.target else {
            return false;
        };
        self.editor
            .as_ref()
            .and_then(|e| {
                e.info
                    .annotations
                    .iter()
                    .find(|a| a.page == s.page && a.index == index)
            })
            .is_some_and(|a| a.contents.as_deref().unwrap_or("") != s.draft)
    }
    pub(super) fn finish_comment(&mut self) -> bool {
        if !self.comment_changed() {
            return true;
        }
        let Some(s) = self.selected.clone() else {
            return true;
        };
        let Target::Comment(index) = s.target else {
            return true;
        };
        if !self.permit() {
            return false;
        }
        let Some(e) = &mut self.editor else {
            return false;
        };
        let result = e.set_comment(s.page, index, &s.draft);
        self.changed(result, s.page)
    }
    pub(super) fn place_comment(&mut self, page: usize, p: &PageInfo, point: Pos2) {
        if !self.finish_text() || !self.permit() {
            return;
        }
        let position = p.view_to_user(point.x, point.y);
        let Some(e) = &mut self.editor else { return };
        match e.add_comment(page, position.map(f64::from), "") {
            Ok(index) => {
                self.refresh(page);
                self.selected = self.selection_for(page, Target::Comment(index));
            }
            Err(error) => self.message = Some(error),
        }
    }
    pub(super) fn comments_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Commentaires");
        ui.add_space(10.);
        if let Some(s) = self
            .selected
            .clone()
            .filter(|s| matches!(s.target, Target::Comment(_)))
        {
            ui.label(format!("Page {}", s.page + 1));
            let mut draft = s.draft;
            if ui
                .add(
                    egui::TextEdit::multiline(&mut draft)
                        .id_salt(("comment-text", s.page))
                        .desired_width(f32::INFINITY)
                        .desired_rows(7)
                        .hint_text("Écrivez votre commentaire…"),
                )
                .changed()
            {
                if let Some(s) = &mut self.selected {
                    s.draft = draft;
                }
            }
            ui.add_space(8.);
            if icons::small(ui, Icon::Apply, "Valider").clicked() {
                self.finish_comment();
            }
            if icons::small(ui, Icon::Delete, "Supprimer").clicked() {
                self.delete();
            }
            ui.separator();
        } else {
            ui.label("Cliquez dans la page pour ajouter une note.");
            ui.add_space(8.);
        }
        let notes = self
            .editor
            .as_ref()
            .map(|e| {
                e.info
                    .annotations
                    .iter()
                    .filter(|a| a.subtype == "Text" && a.in_reply_to.is_none())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for note in notes {
            let label = format!(
                "Page {} · {}",
                note.page + 1,
                note.contents
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Nouvelle note")
                    .chars()
                    .take(80)
                    .collect::<String>()
            );
            if ui
                .add(
                    egui::Button::new(label)
                        .wrap_mode(egui::TextWrapMode::Wrap)
                        .min_size(Vec2::new(ui.available_width(), 42.)),
                )
                .clicked()
                && self.finish_text()
            {
                self.selected = self.selection_for(note.page, Target::Comment(note.index));
                self.jump = Some(note.page);
            }
        }
    }
}
