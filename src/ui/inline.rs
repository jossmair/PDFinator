//! Inline layout adapted from PdfCraft 0.9 edit_text_ui.rs (MIT OR Apache-2.0).
//! Source glyphs are removed on a disposable COS clone; the real page background is retained.
use super::{Selection, Target, raster::Background};
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use pdfcraft_edit::{TextBlock, TextLine};
use pdfcraft_render::PageInfo;
use std::{collections::HashMap, sync::Arc};

pub struct Inline {
    pub selection: Selection,
    pub original: String,
    pub source: Option<TextBlock>,
    pub lines: Vec<TextLine>,
    pub background: Background,
    pub fonts: HashMap<String, String>,
    pub focus: bool,
    cursor: Option<egui::text::CCursorRange>,
    pub committed: bool,
    pub view: PageInfo,
    pub screen: Rect,
    pub reflow: bool,
}
impl Inline {
    pub fn new(
        ctx: &egui::Context,
        editor: &pdfinator::document::Editor,
        selection: Selection,
        scale: f32,
    ) -> Result<Self, String> {
        let page = selection.page;
        let source = match selection.target {
            Target::Block(i) => editor.blocks(page)?.get(i).cloned(),
            _ => None,
        };
        let lines = pdfcraft_edit::text_lines(&editor.doc, page).map_err(|e| e.to_string())?;
        let bytes = editor.preview_without(
            page,
            match selection.target {
                Target::Block(i) => Some(i),
                _ => None,
            },
            match selection.target {
                Target::Added(i) => Some(i),
                _ => None,
            },
        )?;
        let mut definitions = ctx.fonts(|f| f.definitions().clone());
        definitions
            .font_data
            .retain(|name, _| !name.starts_with("document-"));
        definitions.families.retain(
            |family, _| !matches!(family,FontFamily::Name(name)if name.starts_with("document-")),
        );
        let programs =
            pdfcraft_edit::text::text_editor_fonts(&editor.doc, page).map_err(|e| e.to_string())?;
        let mut fonts = HashMap::new();
        for (i, p) in programs.iter().enumerate() {
            let name = format!("document-{i}");
            definitions.font_data.insert(
                name.clone(),
                Arc::new(egui::FontData::from_owned(p.bytes.clone())),
            );
            fonts.insert(p.resource.clone(), name);
        }
        for p in &programs {
            if let Some(name) = fonts.get(&p.resource) {
                let mut stack = vec![name.clone()];
                stack.extend(
                    programs
                        .iter()
                        .filter(|f| f.face == p.face && f.resource != p.resource)
                        .filter_map(|f| fonts.get(&f.resource).cloned()),
                );
                if let Some(fallback) = definitions.families.get(&FontFamily::Proportional) {
                    stack.extend(fallback.iter().cloned());
                }
                definitions
                    .families
                    .insert(FontFamily::Name(name.clone().into()), stack);
            }
        }
        if !fonts.is_empty() {
            ctx.set_fonts(definitions);
        }
        Ok(Self {
            original: selection.draft.clone(),
            selection,
            source,
            lines,
            background: Background::new(bytes, page, scale),
            fonts,
            focus: true,
            cursor: None,
            committed: false,
            view: editor
                .info
                .pages
                .get(page)
                .cloned()
                .ok_or("Page introuvable.")?,
            screen: Rect::NOTHING,
            reflow: false,
        })
    }
    pub fn changed(&self) -> bool {
        self.selection.draft != self.original
            || self.selection.style_changed()
            || (matches!(self.selection.target, Target::New) && !self.selection.draft.is_empty())
    }
    fn fonts_ready(&self, ctx: &egui::Context) -> bool {
        let ready = ctx.fonts(|f| {
            let definitions = f.definitions();
            self.fonts.values().all(|name| {
                definitions
                    .families
                    .get(&FontFamily::Name(name.clone().into()))
                    .is_some_and(|stack| {
                        !stack.is_empty()
                            && stack
                                .iter()
                                .all(|font| definitions.font_data.contains_key(font))
                    })
            })
        });
        if !ready {
            ctx.request_repaint();
        }
        ready
    }
    /// set_fonts takes effect at the next frame. Drag previews can be created
    /// immediately after committing a new face, before that family is bound.
    pub fn layout(
        &self,
        ui: &egui::Ui,
        text: &str,
        zoom: f32,
        width: f32,
    ) -> Option<Arc<egui::Galley>> {
        self.fonts_ready(ui.ctx())
            .then(|| self.layout_ready(ui, text, zoom, width))
    }
    fn layout_ready(&self, ui: &egui::Ui, text: &str, zoom: f32, width: f32) -> Arc<egui::Galley> {
        let s = &self.selection;
        if self.reflow
            || !s.rich.is_empty()
            || s.typing != Default::default()
            || s.line_spacing.is_some()
        {
            return self.formatted_layout(ui, text, zoom, width);
        }
        let color = |c: [f64; 3]| {
            Color32::from_rgb(
                (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                (c[2].clamp(0.0, 1.0) * 255.0) as u8,
            )
        };
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = if text == self.original && !s.style_changed() {
            f32::INFINITY
        } else {
            width
        };
        let font_for = |resource: &str, size: f64, bold: bool| {
            let family = s
                .resource
                .as_deref()
                .and_then(|r| self.fonts.get(r))
                .or_else(|| {
                    if s.family.is_none() && s.bold == s.original_bold {
                        self.fonts.get(resource)
                    } else {
                        None
                    }
                })
                .map(|n| FontFamily::Name(n.clone().into()))
                .unwrap_or_else(|| match s.family {
                    Some(pdfcraft_edit::Family::Courier) => FontFamily::Monospace,
                    _ => FontFamily::Name(if bold { "Interface-Bold" } else { "Interface" }.into()),
                });
            FontId::new((size as f32 * zoom).clamp(1.0, 300.0), family)
        };
        if let Some(block) = &self.source {
            for (range, index) in block.edited_spans(text) {
                if let Some(line) = self.lines.get(index) {
                    let size = if s.size != s.original_size {
                        s.size
                    } else {
                        line.size
                    };
                    let bold = if s.bold != s.original_bold {
                        s.bold
                    } else {
                        line.bold
                    };
                    let font = font_for(&line.font, size, bold);
                    let natural = ui.fonts_mut(|f| {
                        f.layout_no_wrap(line.text.clone(), font.clone(), Color32::BLACK)
                            .size()
                            .x
                    });
                    let target =
                        ((line.rect[2] - line.rect[0]) * size / line.size.max(0.01)) as f32 * zoom;
                    let spacing = if s.family.is_none() && s.resource.is_none() {
                        (target - natural)
                            / line.text.chars().count().saturating_sub(1).max(1) as f32
                    } else {
                        0.0
                    };
                    job.append(
                        text.get(range).unwrap_or_default(),
                        0.0,
                        egui::TextFormat {
                            font_id: font,
                            color: color(if s.color != s.original_color {
                                s.color.map(f64::from)
                            } else {
                                line.color
                            }),
                            extra_letter_spacing: spacing,
                            italics: line.italic,
                            ..Default::default()
                        },
                    );
                }
            }
        } else {
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: font_for("", s.size, s.bold),
                    color: color(s.color.map(f64::from)),
                    ..Default::default()
                },
            );
        }
        let mut galley = ui.fonts_mut(|f| f.layout_job(job));
        if self.view.rotation != 0 {
            return galley;
        }
        let Some(block) = &self.source else {
            return galley;
        };
        let sources: Vec<_> = block
            .rows
            .iter()
            .filter_map(|row| {
                let first = row.first().and_then(|i| self.lines.get(*i))?;
                let rect = row
                    .iter()
                    .filter_map(|i| self.lines.get(*i))
                    .map(|l| l.rect)
                    .reduce(|a, b| {
                        [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[2].max(b[2]),
                            a[3].max(b[3]),
                        ]
                    })?;
                Some((first.baseline()[1], rect))
            })
            .collect();
        let Some((first_baseline, _)) = sources.first().copied() else {
            return galley;
        };
        let leading = sources
            .windows(2)
            .map(|r| (r[0].0 - r[1].0).abs())
            .rfind(|v| *v > 0.0)
            .unwrap_or(block.size * 1.2);
        let unit =
            self.view.height as f64 / (self.view.crop[3] - self.view.crop[1]).max(0.001) as f64;
        let scale = zoom * unit as f32;
        let g = Arc::make_mut(&mut galley);
        let mut bounds = Rect::NOTHING;
        let mut meshes = Rect::NOTHING;
        for (index, placed) in g.rows.iter_mut().enumerate() {
            let (last_base, last_rect) = sources
                .last()
                .copied()
                .unwrap_or((first_baseline, block.rect));
            let (base, r) = sources.get(index).copied().unwrap_or((
                last_base - index.saturating_add(1).saturating_sub(sources.len()) as f64 * leading,
                last_rect,
            ));
            let ratio = if s.size != s.original_size {
                s.size / s.original_size.max(0.01)
            } else {
                1.0
            };
            let ascent =
                ((r[3] - sources.get(index).map_or(last_base, |s| s.0)) * ratio) as f32 * scale;
            let descent =
                ((sources.get(index).map_or(last_base, |s| s.0) - r[1]) * ratio) as f32 * scale;
            let row = Arc::make_mut(&mut placed.row);
            placed.pos.y = (block.rect[3] - base) as f32 * scale - ascent;
            placed.pos.x = (r[0] - block.rect[0]) as f32 * scale;
            for glyph in &mut row.glyphs {
                let dy = ascent - glyph.pos.y;
                glyph.pos.y = ascent;
                glyph.font_ascent = ascent;
                glyph.font_height = ascent + descent;
                glyph.line_height = ascent + descent;
                if !glyph.uv_rect.is_nothing() {
                    for vertex in row
                        .visuals
                        .mesh
                        .vertices
                        .iter_mut()
                        .skip(glyph.first_vertex as usize)
                        .take(4)
                    {
                        vertex.pos.y += dy;
                    }
                }
            }
            row.size.y = (ascent + descent).max(1.0);
            row.visuals.mesh_bounds = row
                .visuals
                .mesh
                .vertices
                .iter()
                .fold(Rect::NOTHING, |r, v| {
                    r.union(Rect::from_min_max(v.pos, v.pos))
                });
            bounds = bounds.union(Rect::from_min_size(placed.pos, row.size));
            meshes = meshes.union(row.visuals.mesh_bounds.translate(placed.pos.to_vec2()));
        }
        g.rect = Rect::from_min_max(
            Pos2::ZERO,
            egui::pos2(bounds.right().max(0.0), bounds.bottom().max(0.0)),
        );
        g.mesh_bounds = meshes;
        galley
    }
    fn formatted_layout(
        &self,
        ui: &egui::Ui,
        text: &str,
        zoom: f32,
        width: f32,
    ) -> Arc<egui::Galley> {
        let s = &self.selection;
        let unit = if self.view.rotation.is_multiple_of(180) {
            self.view.width
        } else {
            self.view.height
        } / (self.view.crop[2] - self.view.crop[0]).max(0.001);
        let source = self
            .source
            .as_ref()
            .map(|b| b.edited_spans(text))
            .unwrap_or_default();
        let mut edges = vec![0, text.len()];
        for (r, _) in &source {
            edges.extend([r.start, r.end]);
        }
        for span in &s.rich {
            edges.extend([
                span.range.start.min(text.len()),
                span.range.end.min(text.len()),
            ]);
        }
        edges.sort_unstable();
        edges.dedup();
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        for edge in edges.windows(2) {
            let (start, end) = (edge[0], edge[1]);
            let Some(part) = text.get(start..end) else {
                continue;
            };
            let line = source
                .iter()
                .find(|(r, _)| r.contains(&start))
                .and_then(|(_, i)| self.lines.get(*i));
            let format = s
                .rich
                .iter()
                .find(|r| r.range.contains(&start))
                .map(|r| r.format.clone())
                .unwrap_or_else(|| {
                    if s.draft.is_empty() {
                        s.typing.clone()
                    } else {
                        Default::default()
                    }
                });
            let size = format
                .size
                .unwrap_or_else(|| line.map_or(s.size, |l| l.size));
            let rgb = format
                .color
                .unwrap_or_else(|| line.map_or(s.color.map(f64::from), |l| l.color));
            let color = Color32::from_rgb(
                (rgb[0] * 255.) as u8,
                (rgb[1] * 255.) as u8,
                (rgb[2] * 255.) as u8,
            );
            let bold = format
                .bold
                .unwrap_or_else(|| line.map_or(s.bold, |l| l.bold));
            let italic = format
                .italic
                .unwrap_or_else(|| line.map_or(s.italic, |l| l.italic));
            let key = format
                .resource
                .as_deref()
                .or(s.resource.as_deref())
                .or_else(|| line.map(|l| l.font.as_str()));
            let embedded = key.and_then(|r| self.fonts.get(r));
            let family = embedded
                .map(|n| FontFamily::Name(n.clone().into()))
                .unwrap_or_else(|| {
                    FontFamily::Name(if bold { "Interface-Bold" } else { "Interface" }.into())
                });
            let script = format.script.unwrap_or(0);
            let font = FontId::new(
                (size as f32 * zoom * unit * if script == 0 { 1. } else { 0.7 }).clamp(1., 400.),
                family,
            );
            let spacing = if format.resource.is_none() && format.family.is_none() {
                line.map_or(0., |l| {
                    let natural = ui.fonts_mut(|f| {
                        f.layout_no_wrap(l.text.clone(), font.clone(), color)
                            .size()
                            .x
                    });
                    let target =
                        ((l.rect[2] - l.rect[0]) * size / l.size.max(0.001)) as f32 * zoom * unit;
                    (target - natural) / l.text.chars().count().saturating_sub(1).max(1) as f32
                })
            } else {
                0.
            };
            job.append(
                part,
                0.,
                egui::TextFormat {
                    font_id: font,
                    color,
                    italics: italic && embedded.is_none(),
                    extra_letter_spacing: spacing,
                    underline: if format.underline.unwrap_or(false) {
                        egui::Stroke::new((zoom * 0.7).max(1.), color)
                    } else {
                        egui::Stroke::NONE
                    },
                    strikethrough: if format.strike.unwrap_or(false) {
                        egui::Stroke::new((zoom * 0.7).max(1.), color)
                    } else {
                        egui::Stroke::NONE
                    },
                    background: if format.highlight.unwrap_or(format.background.is_some()) {
                        let c = format.background.unwrap_or([1., 0.9, 0.25]);
                        Color32::from_rgb(
                            (c[0] * 255.) as u8,
                            (c[1] * 255.) as u8,
                            (c[2] * 255.) as u8,
                        )
                    } else {
                        Color32::TRANSPARENT
                    },
                    valign: match script {
                        1 => egui::Align::TOP,
                        -1 => egui::Align::BOTTOM,
                        _ => egui::Align::BOTTOM,
                    },
                    ..Default::default()
                },
            );
        }
        let mut galley = ui.fonts_mut(|f| f.layout_job(job));
        if width.is_finite() || s.line_spacing.is_some() {
            let g = Arc::make_mut(&mut galley);
            let baseline = |r: &egui::epaint::text::PlacedRow| {
                r.pos.y + r.row.glyphs.first().map_or(r.row.size.y * 0.8, |g| g.pos.y)
            };
            let first = g.rows.first().map_or(0., baseline);
            let mut next = first;
            let mut bounds = Rect::NOTHING;
            let mut meshes = Rect::NOTHING;
            for row in &mut g.rows {
                if let Some(spacing) = s.line_spacing {
                    row.pos.y += next - baseline(row);
                    let size = s.size as f32 * zoom * unit;
                    next += spacing as f32 * size;
                }
                row.pos.x = match s.align.unwrap_or(pdfcraft_edit::Align::Left) {
                    pdfcraft_edit::Align::Center if width.is_finite() => {
                        (width - row.row.size.x).max(0.) / 2.
                    }
                    pdfcraft_edit::Align::Right if width.is_finite() => {
                        (width - row.row.size.x).max(0.)
                    }
                    _ => 0.,
                };
                bounds = bounds.union(Rect::from_min_size(row.pos, row.row.size));
                meshes = meshes.union(row.row.visuals.mesh_bounds.translate(row.pos.to_vec2()));
            }
            g.rect.max.y = bounds.bottom().max(0.);
            if width.is_finite() {
                g.rect.max.x = g.rect.max.x.max(width);
            }
            g.mesh_bounds = meshes;
        }
        galley
    }
    pub(super) fn toggle_bullets(&mut self) {
        let old = self.selection.draft.clone();
        let cursor = self.cursor.unwrap_or_else(|| {
            egui::text::CCursorRange::one(egui::text::CCursor::new(old.chars().count()))
        });
        let byte = |n| {
            old.char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(old.len()))
                .nth(n)
                .unwrap_or(old.len())
        };
        let mut positions = [byte(cursor.primary.index.0), byte(cursor.secondary.index.0)];
        pdfinator::rich::toggle_bullets(
            &mut self.selection.draft,
            &mut self.selection.rich,
            self.selection.text_range.clone(),
            &mut positions,
        );
        let text = &self.selection.draft;
        let mut cursor = cursor;
        cursor.primary.index = egui::text::CharIndex(text[..positions[0]].chars().count());
        cursor.secondary.index = egui::text::CharIndex(text[..positions[1]].chars().count());
        self.cursor = Some(cursor);
        self.selection.text_range = (!cursor.is_empty())
            .then(|| positions[0].min(positions[1])..positions[0].max(positions[1]));
        self.focus = true;
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, page_rect: Rect, zoom: f32) -> Option<Rect> {
        self.background.texture.as_ref()?;
        if !self.fonts_ready(ui.ctx()) {
            return None;
        }
        let local = self.selection.view_rect(&self.view);
        let origin = page_rect.min + local.min.to_vec2() * zoom;
        let mut width = local.width().max(4.0) * zoom;
        let text = self.selection.draft.clone();
        if self.source.as_ref().is_some_and(|b| b.rows.len() == 1)
            && text != self.original
            && !text.contains('\n')
        {
            let natural = self.layout_ready(ui, &text, zoom, f32::INFINITY).size().x;
            width = width
                .max(natural)
                .min((page_rect.right() - origin.x - 12.).max(width));
        }
        let galley = self.layout_ready(ui, &text, zoom, width);
        self.screen = Rect::from_min_size(
            origin,
            Vec2::new(
                width.max(galley.size().x),
                galley.size().y.max(local.height() * zoom),
            ),
        );
        if self.committed {
            ui.painter().galley(origin, galley, Color32::BLACK);
            return Some(self.screen);
        }
        // The UI text widget collapses a selection when a property control takes
        // focus. Restore its last focused caret/selection before returning to it.
        let id = egui::Id::new("direct-text");
        let mut buffer = self.selection.draft.clone();
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
            self.layout_ready(ui, text.as_str(), zoom, wrap)
        };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("inline-source")
                .max_rect(Rect::from_min_size(origin, Vec2::new(width, 10000.0)))
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        child.set_clip_rect(ui.clip_rect().intersect(page_rect));
        let mut output = egui::TextEdit::multiline(&mut buffer)
            .id(id)
            .margin(egui::Margin::ZERO)
            .frame(egui::Frame::NONE)
            .desired_width(width)
            .desired_rows(1)
            .return_key(egui::KeyboardShortcut::new(
                egui::Modifiers::SHIFT,
                egui::Key::Enter,
            ))
            .layouter(&mut layouter)
            .show(&mut child);
        if self.focus {
            output.response.request_focus();
            if !output.response.changed()
                && let Some(cursor) = self.cursor
            {
                output.state.cursor.set_char_range(Some(cursor));
                output.state.clone().store(ui.ctx(), id);
                output.cursor_range = Some(cursor);
            }
            self.focus = false;
        }
        if output.response.changed() {
            let was_empty = self.selection.draft.is_empty();
            pdfinator::rich::remap(&mut self.selection.rich, &self.selection.draft, &buffer);
            if was_empty {
                pdfinator::rich::apply(
                    &mut self.selection.rich,
                    0..buffer.len(),
                    &self.selection.typing,
                );
            }
            self.selection.draft = buffer;
        }
        if output.response.has_focus() {
            if let Some(range) = output.cursor_range {
                self.cursor = Some(range);
                self.selection.text_range = if range.is_empty() {
                    None
                } else {
                    pdfinator::rich::byte_range(
                        &self.selection.draft,
                        range.as_sorted_char_range().start.0..range.as_sorted_char_range().end.0,
                    )
                };
            }
        }
        Some(self.screen)
    }
}
