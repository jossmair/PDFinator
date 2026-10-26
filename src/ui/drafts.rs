//! Versioned recovery of the actual editing state, not only its plain text.
use super::*;
use serde::{Deserialize, Serialize};

pub(super) mod family {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &Option<Family>,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        v.map(Family::label).serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Option<Family>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|v| match v.as_str() {
                "Helvetica" => Ok(Family::Helvetica),
                "Times" => Ok(Family::Times),
                "Courier" => Ok(Family::Courier),
                _ => Err(serde::de::Error::custom(
                    "Famille inconnue dans le brouillon",
                )),
            })
            .transpose()
    }
}
pub(super) mod align {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &Option<Align>,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        v.map(|a| match a {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        })
        .serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Option<Align>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|v| match v.as_str() {
                "left" => Ok(Align::Left),
                "center" => Ok(Align::Center),
                "right" => Ok(Align::Right),
                "justify" => Ok(Align::Justify),
                _ => Err(serde::de::Error::custom(
                    "Alignement inconnu dans le brouillon",
                )),
            })
            .transpose()
    }
}
pub(super) mod fonts {
    use super::*;
    #[derive(Serialize, Deserialize)]
    struct SavedFont {
        key: String,
        name: String,
        family: String,
        bold: bool,
        italic: bool,
        bytes: Vec<u8>,
    }
    pub fn serialize<S: serde::Serializer>(
        fonts: &[(String, typography::FontFace)],
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let values: std::result::Result<Vec<_>, S::Error> = fonts
            .iter()
            .map(|(key, f)| {
                Ok(SavedFont {
                    key: key.clone(),
                    name: f.name.clone(),
                    family: f.family.clone(),
                    bold: f.bold,
                    italic: f.italic,
                    bytes: f.bytes().map_err(serde::ser::Error::custom)?,
                })
            })
            .collect();
        values?.serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Vec<(String, typography::FontFace)>, D::Error> {
        let fonts = Vec::<SavedFont>::deserialize(d)?;
        if fonts.len() > 32 || fonts.iter().map(|f| f.bytes.len()).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err(serde::de::Error::custom(
                "Polices du brouillon trop volumineuses",
            ));
        }
        fonts
            .into_iter()
            .map(|f| {
                if ttf_parser::Face::parse(&f.bytes, 0).is_err() {
                    return Err(serde::de::Error::custom("Police de récupération invalide"));
                }
                Ok((
                    f.key,
                    typography::FontFace {
                        name: f.name,
                        family: f.family,
                        bold: f.bold,
                        italic: f.italic,
                        path: None,
                        bundled: None,
                        cache: Arc::new(std::sync::OnceLock::from(f.bytes)),
                    },
                ))
            })
            .collect()
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Snapshot {
    version: u32,
    selection: Selection,
    original: String,
    caret: Option<[usize; 2]>,
    affinity: [bool; 2],
    caret_x: Option<f32>,
    reflow: bool,
    zoom: f32,
    scroll: [f32; 2],
    collapsed: bool,
}
impl Snapshot {
    pub fn capture(inline: &inline::Inline, zoom: f32, scroll: Vec2, collapsed: bool) -> Self {
        Self {
            version: 1,
            selection: inline.selection.clone(),
            original: inline.original.clone(),
            caret: inline
                .cursor
                .map(|c| [c.primary.index.0, c.secondary.index.0]),
            reflow: inline.reflow,
            affinity: inline
                .cursor
                .map(|c| [c.primary.prefer_next_row, c.secondary.prefer_next_row])
                .unwrap_or([false; 2]),
            caret_x: inline.cursor.and_then(|c| c.h_pos),
            zoom,
            scroll: [scroll.x, scroll.y],
            collapsed,
        }
    }
    pub fn comment(selection: Selection, zoom: f32, scroll: Vec2, collapsed: bool) -> Self {
        Self {
            version: 1,
            selection,
            original: String::new(),
            caret: None,
            reflow: false,
            affinity: [false; 2],
            caret_x: None,
            zoom,
            scroll: [scroll.x, scroll.y],
            collapsed,
        }
    }
    pub fn pending(self) -> Result<pdfinator::recovery::PendingState> {
        let mut fingerprint = self.clone();
        let font_keys: Vec<_> = fingerprint
            .selection
            .font_bindings
            .iter()
            .map(|(k, f)| {
                (
                    k.clone(),
                    f.name.clone(),
                    f.family.clone(),
                    f.bold,
                    f.italic,
                )
            })
            .collect();
        fingerprint.selection.font_bindings.clear();
        let key = serde_json::to_string(&(fingerprint, font_keys)).map_err(|e| e.to_string())?;
        Ok(pdfinator::recovery::PendingState {
            key,
            encode: Arc::new(move || serde_json::to_string(&self).map_err(|e| e.to_string())),
        })
    }
    pub fn restore(self, app: &mut App, ctx: &egui::Context) -> Result<()> {
        if self.version != 1 {
            return Err("Version de brouillon non prise en charge.".into());
        }
        let e = app
            .editor
            .as_ref()
            .ok_or("Document récupéré indisponible")?;
        let s = &self.selection;
        let page = e
            .info
            .pages
            .get(s.page)
            .ok_or("Page du brouillon introuvable")?;
        let valid_target = match s.target {
            Target::Block(i) => e.blocks(s.page)?.get(i).is_some(),
            Target::Added(i) => e.added(s.page).get(i).is_some(),
            Target::New => true,
            Target::Comment(i) => e
                .info
                .annotations
                .iter()
                .any(|a| a.page == s.page && a.index == i),
            _ => false,
        };
        if !valid_target
            || s.draft.len() > 4_000_000
            || !self.zoom.is_finite()
            || !(0.2..=4.).contains(&self.zoom)
            || self.scroll.iter().any(|v| !v.is_finite() || *v < 0.)
            || (!matches!(s.target, Target::Comment(_)) && !s.data(page).valid())
        {
            return Err(
                "Les réglages du brouillon sont invalides. Le texte reste disponible.".into(),
            );
        }
        let comment = matches!(s.target, Target::Comment(_));
        let mut restored = if comment {
            None
        } else {
            Some(inline::Inline::new(
                ctx,
                e,
                s.clone(),
                self.zoom.clamp(0.25, 4.),
            )?)
        };
        if let Some(inline) = &mut restored {
            inline.original = self.original;
            inline.reflow = self.reflow;
            inline.cursor = self.caret.map(|c| {
                let mut primary = egui::text::CCursor::new(c[0].min(s.draft.chars().count()));
                let mut secondary = egui::text::CCursor::new(c[1].min(s.draft.chars().count()));
                primary.prefer_next_row = self.affinity[0];
                secondary.prefer_next_row = self.affinity[1];
                egui::text::CCursorRange {
                    primary,
                    secondary,
                    h_pos: self.caret_x.filter(|v| v.is_finite()),
                }
            });
            let mut definitions = ctx.fonts(|f| f.definitions().clone());
            for (key, font) in &s.font_bindings {
                let name = format!("recovered-{}-{}", e.instance, font.name);
                definitions.font_data.insert(
                    name.clone(),
                    Arc::new(egui::FontData::from_owned(font.bytes()?)),
                );
                let mut stack = vec![name.clone()];
                stack.extend(
                    definitions
                        .families
                        .get(&egui::FontFamily::Proportional)
                        .cloned()
                        .unwrap_or_default(),
                );
                definitions
                    .families
                    .insert(egui::FontFamily::Name(name.clone().into()), stack);
                inline.fonts.insert(key.clone(), name);
            }
            if !s.font_bindings.is_empty() {
                ctx.set_fonts(definitions);
            }
        }
        app.page = s.page;
        app.page_destination = s.page + 1;
        app.jump = None;
        app.zoom = self.zoom;
        app.fit = Fit::Manual;
        app.scroll_offset = Vec2::new(self.scroll[0], self.scroll[1]);
        app.properties_collapsed = self.collapsed;
        app.workspace = if comment {
            Workspace::Read
        } else {
            Workspace::Edit
        };
        app.tool = if comment { Tool::Comment } else { Tool::Edit };
        app.selected = Some(self.selection);
        app.inline = restored;
        Ok(())
    }
}
impl App {
    pub(super) fn failed_recovery_open(&mut self) {
        if let Some(entry) = self.work.recovering_entry.take() {
            self.work.journal.entries.push(entry);
            self.work.show_recovery = true;
        }
        self.work.recovering_from = None;
        self.work.pending_restore = None;
    }
    pub(super) fn restore_pending_draft(&mut self, ctx: &egui::Context) {
        let Some((state, text)) = self.work.pending_restore.take() else {
            return;
        };
        let mut restored = true;
        if let Some(state) = state {
            let result = serde_json::from_str::<Snapshot>(&state)
                .map_err(|e| e.to_string())
                .and_then(|s| s.restore(self, ctx));
            if let Err(error) = result {
                restored = false;
                self.message = Some(format!(
                    "Le PDF est récupéré, mais le brouillon n’a pas pu être replacé : {error}"
                ));
                self.work.recovered_draft = Some(text);
            }
        } else if !text.is_empty() {
            self.work.recovered_draft = Some(text);
        }
        if restored {
            if let Some(old) = self.work.recovering_from.take()
                && let Some(e) = &self.editor
            {
                self.work.journal.adopt(e.instance, old);
            }
            self.work.recovering_entry = None;
        } else {
            // Keep the original full checkpoint on disk, including unknown schema data.
            self.work.recovering_from = None;
            if let Some(entry) = self.work.recovering_entry.take() {
                self.work.journal.entries.push(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prime(ctx: &egui::Context) {
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
    }
    fn app(ctx: &egui::Context) -> App {
        let d = pdfcraft_create::blank(595., 842., 2).unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        e.add_text(
            1,
            AddedText {
                rect: [40., 600., 520., 760.],
                text: "Texte initial".into(),
                size: 14.,
                ..Default::default()
            },
        )
        .unwrap();
        let mut app = App::from_context(ctx, None);
        prime(ctx);
        app.install_opened(e);
        app.workspace = Workspace::Edit;
        app.tool = Tool::Edit;
        app.page = 1;
        let s = app.selection_for(1, Target::Added(0)).unwrap();
        app.begin_text(ctx, s);
        app
    }
    #[test]
    fn recovery_restores_styled_text_fonts_caret_and_layout_without_committing() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let font = app.fonts.entries[0].clone();
        let inline = app.inline.as_mut().unwrap();
        let s = &mut inline.selection;
        s.draft = "• Été : x²\nDeuxième ligne".into();
        s.size = 19.25;
        s.bold = true;
        s.italic = true;
        s.color = [0.2, 0.3, 0.7];
        s.align = Some(Align::Center);
        s.line_spacing = Some(1.7);
        s.rect = [60., 500., 500., 730.];
        s.font_name = font.name.clone();
        s.typing = pdfinator::rich::Format {
            underline: Some(true),
            script: Some(1),
            ..Default::default()
        };
        s.rich = vec![pdfinator::rich::Span {
            range: 0..s.draft.len(),
            format: pdfinator::rich::Format {
                resource: Some("chosen:Inter".into()),
                font_name: Some(font.name.clone()),
                bold: Some(true),
                italic: Some(true),
                underline: Some(true),
                strike: Some(true),
                background: Some([1., 0.9, 0.2]),
                size: Some(19.25),
                ..Default::default()
            },
        }];
        s.text_range = Some(0..s.draft.len());
        s.font_bindings = vec![("chosen:Inter".into(), font)];
        inline.cursor = Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(2),
            egui::text::CCursor::new(7),
        ));
        inline.reflow = true;
        let snapshot = Snapshot::capture(inline, 1.25, Vec2::new(12., 180.), true);
        let expected = serde_json::to_value(&snapshot.selection).unwrap();
        let pending = snapshot.pending().unwrap();
        let bytes = app.editor.as_ref().unwrap().bytes.clone();
        let dir = tempfile::tempdir_in("test-output").unwrap();
        let mut journal = pdfinator::recovery::Journal::start(dir.path()).unwrap();
        journal.observe_state(
            1,
            1,
            bytes.clone(),
            "Cours.pdf".into(),
            "fallback".into(),
            true,
            Some(pending),
        );
        let mut checkpoint = None;
        for _ in 0..250 {
            checkpoint = std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
                .find(|p| p.path().extension().is_some_and(|e| e == "json"));
            if checkpoint.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(checkpoint.is_some(), "Full journal state was not published");
        drop(journal);
        let restored_journal = pdfinator::recovery::Journal::start(dir.path()).unwrap();
        let state = restored_journal.entries[0].metadata.state.clone().unwrap();
        let recovered_ctx = egui::Context::default();
        let mut recovered = App::from_context(&recovered_ctx, None);
        prime(&recovered_ctx);
        recovered.install_opened(Editor::from_bytes(bytes.as_ref().clone()).unwrap());
        recovered.work.pending_restore = Some((Some(state), "fallback".into()));
        recovered.restore_pending_draft(&recovered_ctx);
        assert!(recovered.message.is_none(), "{:?}", recovered.message);
        assert!(recovered.work.recovered_draft.is_none());
        assert_eq!(recovered.editor.as_ref().unwrap().bytes, bytes);
        assert_eq!(
            serde_json::to_value(&recovered.inline.as_ref().unwrap().selection).unwrap(),
            expected
        );
        assert!(recovered.inline.as_ref().unwrap().changed());
        assert_eq!(recovered.inline.as_ref().unwrap().original, "Texte initial");
        assert_eq!(
            recovered
                .inline
                .as_ref()
                .unwrap()
                .cursor
                .unwrap()
                .primary
                .index
                .0,
            7
        );
        assert_eq!(
            recovered
                .inline
                .as_ref()
                .unwrap()
                .cursor
                .unwrap()
                .secondary
                .index
                .0,
            2
        );
        assert!(recovered.properties_collapsed);
        assert_eq!(recovered.page, 1);
        assert_eq!(recovered.zoom, 1.25);
        assert_eq!(recovered.scroll_offset, Vec2::new(12., 180.));
        let selection = recovered.inline.as_ref().unwrap().selection.clone();
        let target = recovered.commit_text_selection(&selection).unwrap();
        let e = recovered.editor.as_ref().unwrap();
        let reopened = Editor::from_bytes(e.bytes.as_ref().clone()).unwrap();
        let Target::Block(index) = target else {
            panic!("rich text must become an editable block")
        };
        let data = reopened.text_formatting(1, index).unwrap();
        assert_eq!(data.text, selection.draft);
        assert_eq!(data.base_size, 19.25);
        assert_eq!(data.line_spacing, Some(1.7));
        assert_eq!(data.align.as_deref(), Some("Centre"));
        assert!(
            data.spans
                .iter()
                .any(|s| s.format.strike == Some(true) && s.format.underline == Some(true))
        );
    }
    #[test]
    fn style_only_edits_change_the_checkpoint_key_and_unknown_versions_keep_plain_text() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let first = Snapshot::capture(app.inline.as_ref().unwrap(), 1., Vec2::ZERO, false)
            .pending()
            .unwrap();
        app.inline.as_mut().unwrap().selection.line_spacing = Some(2.);
        let second = Snapshot::capture(app.inline.as_ref().unwrap(), 1., Vec2::ZERO, false)
            .pending()
            .unwrap();
        assert_ne!(first.key, second.key);
        let mut json: serde_json::Value =
            serde_json::from_str(&(second.encode)().unwrap()).unwrap();
        json["version"] = 999.into();
        app.inline = None;
        app.work.pending_restore = Some((Some(json.to_string()), "Texte à conserver".into()));
        app.restore_pending_draft(&ctx);
        assert_eq!(
            app.work.recovered_draft.as_deref(),
            Some("Texte à conserver")
        );
        assert!(app.message.is_some());
        assert!(app.inline.is_none());
    }
    #[test]
    fn legacy_plain_text_checkpoints_remain_readable() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        app.work.pending_restore = Some((None, "Ancien brouillon".into()));
        app.restore_pending_draft(&ctx);
        assert_eq!(
            app.work.recovered_draft.as_deref(),
            Some("Ancien brouillon")
        );
    }
    #[test]
    fn a_comment_draft_is_restored_on_its_original_annotation() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        app.inline = None;
        let index = app
            .editor
            .as_mut()
            .unwrap()
            .add_comment(1, [80., 240.], "Avant")
            .unwrap();
        let mut selection = app.selection_for(1, Target::Comment(index)).unwrap();
        selection.draft = "À préciser : été, cœur et œuf.".into();
        let text = selection.draft.clone();
        let state = (Snapshot::comment(selection, 1.1, Vec2::ZERO, false)
            .pending()
            .unwrap()
            .encode)()
        .unwrap();
        app.work.pending_restore = Some((Some(state), text.clone()));
        app.restore_pending_draft(&ctx);
        assert!(app.comment_changed());
        assert!(app.inline.is_none());
        assert!(app.finish_comment());
        assert!(
            app.editor
                .as_ref()
                .unwrap()
                .info
                .annotations
                .iter()
                .any(|a| a.contents.as_deref() == Some(text.as_str()))
        );
    }
    #[test]
    fn failed_recovery_open_cannot_apply_a_draft_to_the_next_normal_document() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let metadata = pdfinator::recovery::Metadata {
            name: "Cours.pdf".into(),
            draft: "Texte".into(),
            state: Some("styles".into()),
            checkpoint: None,
        };
        app.work.recovering_entry = Some(pdfinator::recovery::Entry {
            pdf: PathBuf::from("missing.pdf"),
            metadata,
        });
        app.work.recovering_from = Some(PathBuf::from("missing.pdf"));
        app.work.pending_restore = Some((Some("styles".into()), "Texte".into()));
        app.failed_recovery_open();
        assert!(app.work.pending_restore.is_none());
        assert!(app.work.recovering_from.is_none());
        assert_eq!(app.work.journal.entries.len(), 1);
        assert!(app.work.show_recovery);
    }
    #[test]
    fn recovering_another_tab_keeps_the_previous_draft_uncommitted_and_editable() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let before = app.editor.as_ref().unwrap().bytes.clone();
        app.inline.as_mut().unwrap().selection.draft = "Premier brouillon non validé".into();
        app.inline.as_mut().unwrap().selection.line_spacing = Some(1.8);
        app.properties_collapsed = true;
        app.work.recovering_entry = Some(pdfinator::recovery::Entry {
            pdf: PathBuf::from("second.pdf"),
            metadata: pdfinator::recovery::Metadata {
                name: "Second.pdf".into(),
                draft: String::new(),
                state: None,
                checkpoint: None,
            },
        });
        app.work.pending_restore = Some((None, String::new()));
        app.work.ready = Some(Editor::from_bytes(before.as_ref().clone()).unwrap());
        app.poll_work(&ctx);
        assert_eq!(app.documents.len(), 2);
        assert!(app.activate_document(0));
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        let inline = app.inline.as_ref().unwrap();
        assert_eq!(inline.selection.draft, "Premier brouillon non validé");
        assert_eq!(inline.selection.line_spacing, Some(1.8));
        assert!(!inline.committed);
        assert!(app.properties_collapsed);
    }
}
