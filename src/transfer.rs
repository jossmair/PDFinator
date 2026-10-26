//! Transfers between documents keep the original PDF fonts, not screen images.
use crate::{
    document::{Editor, Result, guarded},
    rich::{self, Format, TextData},
};
use pdfcraft_cos::{Dict, Document, Object};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct CopiedText {
    font_source: Option<(Document, usize)>,
    pub data: TextData,
    pub view_rect: [f32; 4],
}
#[derive(Clone)]
pub struct CopiedImage {
    source: Document,
    page: usize,
    bytes: std::sync::Arc<Vec<u8>>,
    raster: bool,
    pub view_rect: [f32; 4],
}
impl CopiedImage {
    pub fn clipboard_preview(&self) -> Result<pdfcraft_render::RenderedPage> {
        let side = (self.view_rect[2] - self.view_rect[0])
            .max(self.view_rect[3] - self.view_rect[1])
            .max(1.);
        let mut renderer =
            pdfcraft_render::PageRenderer::new(self.bytes.clone(), Default::default());
        let rendered = renderer.render(pdfcraft_render::RenderRequest {
            page: self.page,
            scale: (2048. / side).min(2.),
            ..Default::default()
        });
        if let Some(error) = &rendered.error {
            return Err(error.clone());
        }
        Ok(rendered)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PastedText {
    Block(usize),
    Added(usize),
}
impl Editor {
    /// Keep native pixels, masks, transforms and clipping in an isolated PDF.
    pub fn copy_image_block(
        &self,
        page: usize,
        target: pdfcraft_edit::ObjectTarget,
    ) -> Result<CopiedImage> {
        if !self.can_copy() {
            return Err("Ce PDF n’autorise pas la copie.".into());
        }
        guarded(|| {
            let object = pdfcraft_edit::editable_objects(&self.doc, page)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|o| o.target == target && o.content_type != "text")
                .ok_or("Sélectionnez une image à copier.")?;
            let p = self.info.pages.get(page).ok_or("Page introuvable.")?;
            let rect = object.rect;
            let view_rect = p.user_to_view(rect[0] as f32, rect[1] as f32);
            let opposite = p.user_to_view(rect[2] as f32, rect[3] as f32);
            let view_rect = [
                view_rect[0].min(opposite[0]),
                view_rect[1].min(opposite[1]),
                view_rect[0].max(opposite[0]),
                view_rect[1].max(opposite[1]),
            ];
            let isolated = self.selection_preview(
                page,
                &[crate::selection::Target::Object(target)],
                true,
                false,
            )?;
            let mut source = Document::open(isolated).map_err(|e| e.to_string())?;
            let source_page = pdfcraft_model::pages(&source)
                .get(page)
                .cloned()
                .ok_or("Page introuvable.")?;
            source
                .update_dict(source_page.obj, |d| {
                    d.set(
                        b"CropBox".to_vec(),
                        Object::Array(rect.map(Object::Real).to_vec()),
                    );
                })
                .map_err(|e| e.to_string())?;
            let raster = match target.kind {
                pdfcraft_edit::ObjectKind::Image => pdfcraft_edit::page_images(&self.doc, page)
                    .map_err(|e| e.to_string())?
                    .get(target.index)
                    .is_some_and(|i| {
                        !i.is_form
                            || i.object.is_some_and(|r| {
                                self.doc.get(r).as_dict().is_some_and(|d| {
                                    matches!(d.get(b"PNRasterImage"), Some(Object::Bool(true)))
                                })
                            })
                    }),
                pdfcraft_edit::ObjectKind::Added => {
                    self.added(page).get(target.index).is_some_and(|c| match c {
                        pdfcraft_edit::Content::Image(i) => {
                            self.doc.get(i.image).as_dict().is_some_and(|d| {
                                d.name(b"Subtype") == Some(b"Image")
                                    || matches!(d.get(b"PNRasterImage"), Some(Object::Bool(true)))
                            })
                        }
                        _ => false,
                    })
                }
                _ => false,
            };
            let bytes = std::sync::Arc::new(
                pdfcraft_cos::write_full(&source, &Default::default())
                    .map_err(|e| e.to_string())?,
            );
            Ok(CopiedImage {
                source,
                page,
                bytes,
                raster,
                view_rect,
            })
        })
    }
    pub fn paste_image_block(
        &mut self,
        page: usize,
        copied: &CopiedImage,
        at: [f32; 2],
    ) -> Result<usize> {
        if at.iter().any(|v| !v.is_finite()) {
            return Err("Position de collage invalide.".into());
        }
        let geometry = self
            .info
            .pages
            .get(page)
            .cloned()
            .ok_or("Page introuvable.")?;
        let w = copied.view_rect[2] - copied.view_rect[0];
        let h = copied.view_rect[3] - copied.view_rect[1];
        if w <= 0. || h <= 0. {
            return Err("Image vide.".into());
        }
        let a = geometry.view_to_user(at[0], at[1] + h);
        let b = geometry.view_to_user(at[0] + w, at[1] + h);
        let c = geometry.view_to_user(at[0], at[1]);
        let mut created = pdfcraft_cos::ObjRef::new(0, 0);
        let mut pasted = 0;
        self.change(false, |doc| {
            let (form, (fw, fh)) =
                pdfcraft_organize::page_as_form(doc, &copied.source, copied.page)
                    .map_err(|e| e.to_string())?;
            if fw <= 0. || fh <= 0. {
                return Err("Image invalide.".into());
            }
            let mut resources = Dict::new();
            let mut images = Dict::new();
            images.set(b"Image".to_vec(), Object::Ref(form));
            resources.set(b"XObject".to_vec(), Object::Dict(images));
            let mut dictionary = Dict::new();
            dictionary.set(b"Type".to_vec(), Object::name("XObject"));
            dictionary.set(b"Subtype".to_vec(), Object::name("Form"));
            dictionary.set(
                b"BBox".to_vec(),
                Object::Array([0., 0., 1., 1.].map(Object::Real).to_vec()),
            );
            dictionary.set(b"Resources".to_vec(), Object::Dict(resources));
            dictionary.set(b"PNRasterImage".to_vec(), Object::Bool(copied.raster));
            created = doc.add(Object::Stream(pdfcraft_cos::Stream::flate(
                dictionary,
                format!("q {} 0 0 {} 0 0 cm /Image Do Q", 1. / fw, 1. / fh).as_bytes(),
            )));
            let p = pdfcraft_model::pages(doc)
                .get(page)
                .cloned()
                .ok_or("Page introuvable.")?;
            let mut resources = p
                .dict
                .get(b"Resources")
                .and_then(|r| doc.dict(r))
                .unwrap_or_default();
            let mut images = resources
                .get(b"XObject")
                .and_then(|r| doc.dict(r))
                .unwrap_or_default();
            let name = format!("PNImage{}", created.num);
            images.set(name.as_bytes().to_vec(), Object::Ref(created));
            resources.set(b"XObject".to_vec(), Object::Dict(images));
            doc.update_dict(p.obj, |d| {
                d.set(b"Resources".to_vec(), Object::Dict(resources));
            })
            .map_err(|e| e.to_string())?;
            pdfcraft_edit::stamp(
                doc,
                page,
                "PDFinatorImagePaste",
                format!(
                    "q {} {} {} {} {} {} cm /{name} Do Q\n",
                    b[0] - a[0],
                    b[1] - a[1],
                    c[0] - a[0],
                    c[1] - a[1],
                    a[0],
                    a[1]
                )
                .into_bytes(),
            )
            .map_err(|e| e.to_string())?;
            pasted = pdfcraft_edit::page_images(doc, page)
                .map_err(|e| e.to_string())?
                .iter()
                .position(|i| i.object == Some(created))
                .ok_or("Image collée introuvable.")?;
            Ok(())
        })?;
        Ok(pasted)
    }
    /// Read-only: the source document and its history never change.
    pub fn copy_text_block(
        &self,
        page: usize,
        block: Option<usize>,
        added: Option<usize>,
    ) -> Result<CopiedText> {
        if !self.can_copy() {
            return Err("Ce PDF n’autorise pas la copie.".into());
        }
        guarded(|| {
            let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
            if let Some(index) = added {
                let pdfcraft_edit::Content::Text(text) = self
                    .added(page)
                    .get(index)
                    .cloned()
                    .ok_or("Bloc introuvable.")?
                else {
                    return Err("Sélectionnez un bloc de texte à copier.".into());
                };
                let mut data = TextData {
                    id: String::new(),
                    text: text.text,
                    rect: text.rect,
                    base_size: text.size,
                    base_color: text.color,
                    base_bold: text.bold,
                    base_italic: text.italic,
                    font_name: text.family.base_font(text.bold, text.italic).into(),
                    align: Some(
                        match text.align {
                            pdfcraft_edit::Align::Center => "Centre",
                            pdfcraft_edit::Align::Right => "Droite",
                            pdfcraft_edit::Align::Justify => "Justifié",
                            _ => "Gauche",
                        }
                        .into(),
                    ),
                    spans: Vec::new(),
                    line_spacing: None,
                };
                rich::apply(
                    &mut data.spans,
                    0..data.text.len(),
                    &Format {
                        family: Some(data.font_name.clone()),
                        size: Some(text.size),
                        color: Some(text.color),
                        bold: Some(text.bold),
                        italic: Some(text.italic),
                        ..Default::default()
                    },
                );
                return Ok(CopiedText {
                    font_source: None,
                    view_rect: [
                        text.rect[0] as f32,
                        geometry.height - text.rect[3] as f32,
                        text.rect[2] as f32,
                        geometry.height - text.rect[1] as f32,
                    ],
                    data,
                });
            }
            let index = block.ok_or("Sélectionnez un bloc de texte à copier.")?;
            let blocks = self.blocks(page)?;
            let b = blocks.get(index).ok_or("Bloc introuvable.")?;
            if b.invisible {
                return Err(
                    "Cette couche de texte est invisible. Sélectionnez un bloc visible.".into(),
                );
            }
            let stored = self.text_formatting(page, index);
            let formatted = stored.is_some();
            let mut data = stored.unwrap_or_else(|| TextData {
                id: String::new(),
                text: b.editor_text.clone(),
                rect: b.rect,
                base_size: b.size,
                base_color: b.color,
                base_bold: b.bold,
                base_italic: b.italic,
                font_name: b.base_font.clone(),
                align: None,
                line_spacing: None,
                spans: Vec::new(),
            });
            let previous = std::mem::take(&mut data.spans);
            let lines = pdfcraft_edit::text_lines(&self.doc, page).map_err(|e| e.to_string())?;
            let format = |line: &pdfcraft_edit::TextLine| Format {
                resource: Some(line.font.clone()),
                font_name: Some(line.base_font.clone()),
                size: Some(line.size),
                color: Some(line.color),
                bold: Some(line.bold),
                italic: Some(line.italic),
                ..Default::default()
            };
            if let Some(first) = b.lines.first().and_then(|i| lines.get(*i)) {
                let mut base = format(first);
                // Owned paragraph metadata records defaults between styled ranges.
                // A decoded PDF line may group differently coloured runs together.
                if formatted {
                    base.size = Some(data.base_size);
                    base.color = Some(data.base_color);
                    base.bold = Some(data.base_bold);
                    base.italic = Some(data.base_italic);
                }
                rich::apply(&mut data.spans, 0..data.text.len(), &base);
            }
            for (range, line) in b.spans.iter().filter(|_| !formatted) {
                if data.text.get(range.clone()).is_some()
                    && let Some(line) = lines.get(*line)
                {
                    rich::apply(&mut data.spans, range.clone(), &format(line));
                }
            }
            for span in previous {
                rich::apply(&mut data.spans, span.range, &span.format);
            }
            // The disposable source contains only the fonts used by this block.
            // No neighbouring content, image or annotation is imported on paste.
            let page_ref = pdfcraft_model::pages(&self.doc)
                .get(page)
                .cloned()
                .ok_or("Page introuvable.")?;
            let resources = page_ref
                .dict
                .get(b"Resources")
                .and_then(|r| self.doc.dict(r))
                .unwrap_or_default();
            let fonts = resources
                .get(b"Font")
                .and_then(|r| self.doc.dict(r))
                .unwrap_or_default();
            let used: HashSet<_> = data
                .spans
                .iter()
                .filter_map(|s| s.format.resource.as_deref())
                .collect();
            let mut selected_fonts = Dict::new();
            for key in used {
                selected_fonts.set(
                    key.as_bytes().to_vec(),
                    fonts
                        .get(key.as_bytes())
                        .cloned()
                        .ok_or("Une police source est introuvable. Aucun bloc copié.")?,
                );
            }
            let mut resources = Dict::new();
            resources.set(b"Font".to_vec(), Object::Dict(selected_fonts));
            let mut source = self.doc.clone();
            source
                .update_dict(page_ref.obj, |d| {
                    d.remove(b"Contents");
                    d.remove(b"Annots");
                    d.set(b"Resources".to_vec(), Object::Dict(resources));
                })
                .map_err(|e| e.to_string())?;
            let a = geometry.user_to_view(data.rect[0] as f32, data.rect[1] as f32);
            let z = geometry.user_to_view(data.rect[2] as f32, data.rect[3] as f32);
            Ok(CopiedText {
                font_source: Some((source, page)),
                view_rect: [
                    a[0].min(z[0]),
                    a[1].min(z[1]),
                    a[0].max(z[0]),
                    a[1].max(z[1]),
                ],
                data,
            })
        })
    }
    /// Paste a real, editable paragraph; one undo restores the destination exactly.
    pub fn paste_text_block(
        &mut self,
        page: usize,
        copied: &CopiedText,
        at: [f32; 2],
    ) -> Result<PastedText> {
        let geometry = self
            .info
            .pages
            .get(page)
            .cloned()
            .ok_or("Page introuvable.")?;
        let mut data = copied.data.clone();
        if !data.valid() || data.text.trim().is_empty() || at.iter().any(|v| !v.is_finite()) {
            return Err("Le bloc à coller est invalide.".into());
        }
        let width = (copied.view_rect[2] - copied.view_rect[0]).max(8.);
        let height = (copied.view_rect[3] - copied.view_rect[1]).max(8.);
        if copied.font_source.is_none() {
            let mut index = 0;
            let text = pdfcraft_edit::AddedText {
                text: data.text,
                rect: [
                    at[0] as f64,
                    (geometry.height - at[1] - height) as f64,
                    (at[0] + width) as f64,
                    (geometry.height - at[1]) as f64,
                ],
                size: data.base_size,
                color: data.base_color,
                bold: data.base_bold,
                italic: data.base_italic,
                family: if data.font_name.contains("Times") {
                    pdfcraft_edit::Family::Times
                } else if data.font_name.contains("Courier") {
                    pdfcraft_edit::Family::Courier
                } else {
                    pdfcraft_edit::Family::Helvetica
                },
                align: match data.align.as_deref() {
                    Some("Centre") => pdfcraft_edit::Align::Center,
                    Some("Droite") => pdfcraft_edit::Align::Right,
                    Some("Justifié") => pdfcraft_edit::Align::Justify,
                    _ => pdfcraft_edit::Align::Left,
                },
            };
            self.change(false, |doc| {
                index = pdfcraft_edit::add_content(doc, page, &pdfcraft_edit::Content::Text(text))
                    .map_err(|e| e.to_string())?;
                Ok(())
            })?;
            return Ok(PastedText::Added(index));
        }
        if geometry.rotation.rem_euclid(360) != 0 {
            return Err("Le collage d’un bloc à styles multiples sur une page tournée n’est pas encore pris en charge. Remettez cette page à l’endroit dans Pages avant de coller. Aucun changement appliqué.".into());
        }
        data.rect = geometry
            .view_rect_to_user([at[0], at[1], at[0] + width, at[1] + height])
            .map(f64::from);
        let mut selected = 0;
        self.change(false, |doc| {
            if let Some((source, source_page)) = &copied.font_source {
                let (form, _) = pdfcraft_organize::page_as_form(doc, source, *source_page)
                    .map_err(|e| e.to_string())?;
                let imported = doc
                    .get(form)
                    .as_dict()
                    .and_then(|d| d.get(b"Resources"))
                    .and_then(|r| doc.dict(r))
                    .and_then(|r| r.get(b"Font").and_then(|f| doc.dict(f)))
                    .unwrap_or_default();
                let p = pdfcraft_model::pages(doc)
                    .get(page)
                    .cloned()
                    .ok_or("Page introuvable.")?;
                let mut resources = p
                    .dict
                    .get(b"Resources")
                    .and_then(|r| doc.dict(r))
                    .unwrap_or_default();
                let mut fonts = resources
                    .get(b"Font")
                    .and_then(|r| doc.dict(r))
                    .unwrap_or_default();
                let mut names = HashMap::new();
                for (key, font) in imported.iter() {
                    let old = String::from_utf8_lossy(key).into_owned();
                    let name = format!("PNPaste{}_{}", form.num, old);
                    fonts.set(name.as_bytes().to_vec(), font.clone());
                    names.insert(old, name);
                }
                resources.set(b"Font".to_vec(), Object::Dict(fonts));
                doc.update_dict(p.obj, |d| {
                    d.set(b"Resources".to_vec(), Object::Dict(resources))
                })
                .map_err(|e| e.to_string())?;
                for span in &mut data.spans {
                    if let Some(key) = &span.format.resource {
                        span.format.resource = Some(
                            names
                                .get(key)
                                .cloned()
                                .ok_or("La police du bloc ne peut pas être transférée.")?,
                        );
                    }
                }
                // This form transported font references only and is never drawn.
                doc.free(form);
            }
            data.id = format!(
                "paste-{}",
                doc.object_numbers().last().copied().unwrap_or(0)
            );
            selected = Self::write_formatted_text(doc, page, None, None, &mut data, &[])?;
            Ok(())
        })?;
        Ok(PastedText::Block(selected))
    }
    /// Append complete PDFs in selection order, atomically as one history entry.
    pub fn append_pdfs(&mut self, sources: &[(&str, Vec<u8>)]) -> Result<usize> {
        if sources.is_empty() {
            return Ok(0);
        }
        let mut opened = Vec::new();
        let mut added = 0usize;
        for (name, bytes) in sources {
            let source = Self::from_bytes(bytes.clone()).map_err(|e| format!("{name} : {e}"))?;
            if !source.can_copy() {
                return Err(format!(
                    "{name} : ce PDF n’autorise pas la copie de ses pages."
                ));
            }
            added = added
                .checked_add(source.info.pages.len())
                .ok_or("Trop de pages à ajouter.")?;
            opened.push((*name, source));
        }
        let count = self
            .info
            .pages
            .len()
            .checked_add(added)
            .ok_or("Trop de pages à ajouter.")?;
        self.change_pages(false, count, |doc| {
            let mut at = pdfcraft_organize::page_count(doc).map_err(|e| e.to_string())?;
            for (name, source) in &opened {
                let pages: Vec<_> = (0..source.info.pages.len()).collect();
                pdfcraft_organize::insert_file_pages(
                    doc,
                    &source.doc,
                    name,
                    &pages,
                    at,
                    pdfcraft_organize::FieldConflicts::Rename,
                )
                .map_err(|e| format!("{name} : {e}"))?;
                at += pages.len();
            }
            Ok(())
        })?;
        Ok(added)
    }
}
