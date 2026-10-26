use pdfcraft_cos::{Document, SaveOptions};
use pdfcraft_edit::{AddedImage, AddedText, BlockStyle, Content, TextBlock};
use pdfcraft_render::{DocInfo, PageRenderer, PageText, RenderConfig, RenderRequest, RequestKind};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::Arc,
};

pub type Result<T> = std::result::Result<T, String>;
fn ocr_view_rect(geometry: &pdfcraft_render::PageInfo, rect: [f64; 4]) -> [f32; 4] {
    let a = geometry.user_to_view(rect[0] as f32, rect[1] as f32);
    let b = geometry.user_to_view(rect[2] as f32, rect[3] as f32);
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    ]
}
fn ocr_contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 0.5
        && inner[1] >= outer[1] - 0.5
        && inner[2] <= outer[2] + 0.5
        && inner[3] <= outer[3] + 0.5
}
fn pdfcraft_content_bytes(properties: &pdfcraft_cos::Dict) -> Vec<u8> {
    pdfcraft_content::serialize_ops(&[pdfcraft_content::Op::new(
        "BDC",
        vec![
            pdfcraft_cos::Object::name("P"),
            pdfcraft_cos::Object::Dict(properties.clone()),
        ],
    )])
}

pub fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| {
        "Le moteur ne peut pas traiter cette structure PDF. Le document précédent est conservé."
            .to_string()
    })?
}

#[derive(Clone)]
struct Snapshot {
    doc: Document,
    bytes: Arc<Vec<u8>>,
    revision: u64,
}

pub struct Editor {
    pub instance: u64,
    pub canonical_path: Option<PathBuf>,
    pub recovered_name: Option<String>,
    pub doc: Document,
    pub bytes: Arc<Vec<u8>>,
    pub info: DocInfo,
    pub path: Option<PathBuf>,
    pub signed: bool,
    pub signature_accepted: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    revision: u64,
    saved_revision: u64,
    next_revision: u64,
    saved_bytes: Arc<Vec<u8>>,
}

impl Editor {
    pub fn open(path: &Path) -> Result<Self> {
        let mut editor = Self::from_bytes(crate::files::read(path)?)?;
        editor.path = Some(path.to_owned());
        editor.canonical_path = path.canonicalize().ok();
        Ok(editor)
    }
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        guarded(|| {
            let bytes = Arc::new(bytes);
            let doc = Document::open(bytes.clone()).map_err(|e| {
                format!("Ouverture impossible (document éventuellement protégé) : {e}")
            })?;
            let info = pdfcraft_render::inspect(bytes.clone(), None).map_err(|e| e.to_string())?;
            if info.pages.is_empty() {
                return Err("Ce PDF ne contient aucune page lisible.".into());
            }
            let signed = doc
                .scan_objects()
                .any(|(_, o)| o.as_dict().is_some_and(|d| d.get(b"ByteRange").is_some()));
            Ok(Self {
                instance: next_instance(),
                canonical_path: None,
                recovered_name: None,
                doc,
                saved_bytes: bytes.clone(),
                bytes,
                info,
                path: None,
                signed,
                signature_accepted: false,
                undo: Vec::new(),
                redo: Vec::new(),
                revision: 0,
                saved_revision: 0,
                next_revision: 1,
            })
        })
    }
    pub fn dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| self.recovered_name.clone())
            .unwrap_or_else(|| "Document".into())
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn can_copy(&self) -> bool {
        self.doc.permissions().is_none_or(|p| p.copy())
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            doc: self.doc.clone(),
            bytes: self.bytes.clone(),
            revision: self.revision,
        }
    }
    pub fn change(
        &mut self,
        annotation: bool,
        f: impl FnOnce(&mut Document) -> Result<()>,
    ) -> Result<()> {
        self.change_pages(annotation, self.info.pages.len(), f)
    }
    pub(crate) fn change_pages(
        &mut self,
        annotation: bool,
        expected_pages: usize,
        f: impl FnOnce(&mut Document) -> Result<()>,
    ) -> Result<()> {
        if self.signed && !self.signature_accepted {
            return Err("Ce PDF est signé. Une modification peut invalider la signature : confirmez explicitement avant de modifier.".into());
        }
        if self.doc.permissions().is_some_and(|p| {
            if annotation {
                !p.annotate()
            } else {
                !p.modify()
            }
        }) {
            return Err("Les permissions de ce PDF interdisent cette modification.".into());
        }
        let (doc, bytes, info) = guarded(|| {
            let mut doc = self.doc.clone();
            f(&mut doc)?;
            let bytes = Arc::new(
                pdfcraft_cos::write_incremental(&doc, &SaveOptions::default())
                    .map_err(|e| e.to_string())?,
            );
            let info = pdfcraft_render::inspect(bytes.clone(), None).map_err(|e| e.to_string())?;
            if info.pages.len() != expected_pages {
                return Err(
                    "Le nombre de pages obtenu est incorrect : modification refusée.".into(),
                );
            }
            Ok((doc, bytes, info))
        })?;
        self.undo.push(self.snapshot());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.doc = doc;
        self.bytes = bytes;
        self.info = info;
        self.revision = self.next_revision;
        self.next_revision += 1;
        Ok(())
    }
    pub fn undo(&mut self) -> Result<()> {
        self.history(false)
    }
    /// Change the page's display rotation, retaining its content and annotations.
    pub fn rotate_page(&mut self, page: usize, degrees: i64) -> Result<()> {
        self.change(false, |doc| {
            pdfcraft_organize::rotate_pages(doc, &[page], degrees).map_err(|e| e.to_string())?;
            // Added-item boxes are stored in their original display coordinates;
            // private metadata does not follow /Rotate. Keep all PDF operators and
            // resources intact, then discover these as ordinary text and images
            // using their actual transforms. This prevents stale selection frames
            // and a jump on the next edit, without flattening any page content.
            let added: Vec<_> = pdfcraft_edit::list_added(doc)
                .into_iter()
                .filter(|a| a.page == page)
                .map(|a| a.obj)
                .collect();
            for object in added {
                doc.update_dict(object, |dict| {
                    dict.remove(b"PCMark");
                    dict.remove(b"PCAdded");
                })
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }
    /// Move a page to its zero-based position in the resulting document.
    pub fn move_page(&mut self, page: usize, destination: usize) -> Result<()> {
        let count = self.info.pages.len();
        if page >= count || destination >= count {
            return Err("Cette page n’existe pas dans le document.".into());
        }
        if page == destination {
            return Ok(());
        }
        self.change(false, |doc| {
            pdfcraft_organize::move_pages(doc, &[page], destination).map_err(|e| e.to_string())
        })
    }
    /// Duplicate a page directly after itself, including its annotations.
    pub fn duplicate_page(&mut self, page: usize) -> Result<()> {
        if page >= self.info.pages.len() {
            return Err("Page introuvable.".into());
        }
        self.change_pages(false, self.info.pages.len() + 1, |doc| {
            pdfcraft_organize::duplicate_pages(doc, &[page], Some(page + 1))
                .map_err(|e| e.to_string())
        })
    }
    pub fn delete_page(&mut self, page: usize) -> Result<()> {
        if page >= self.info.pages.len() {
            return Err("Page introuvable.".into());
        }
        if self.info.pages.len() < 2 {
            return Err("Le document doit conserver au moins une page.".into());
        }
        self.change_pages(false, self.info.pages.len() - 1, |doc| {
            pdfcraft_organize::delete_pages(doc, &[page]).map_err(|e| e.to_string())
        })
    }
    /// Insert a blank sheet immediately after the active page. Use its displayed
    /// size, including crop, rotation and UserUnit, so portrait/landscape matches.
    pub fn insert_blank_after(&mut self, page: usize) -> Result<usize> {
        let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
        let unit = pdfcraft_model::pages(&self.doc)
            .get(page)
            .and_then(|p| {
                p.dict
                    .get(b"UserUnit")
                    .and_then(pdfcraft_cos::Object::as_f64)
            })
            .unwrap_or(1.);
        if !unit.is_finite() || unit <= 0. {
            return Err("L’échelle de cette page n’est pas prise en charge.".into());
        }
        let (width, height) = (geometry.width as f64 / unit, geometry.height as f64 / unit);
        let at = page + 1;
        self.change_pages(false, self.info.pages.len() + 1, |doc| {
            let inserted = pdfcraft_organize::insert_blank_page(doc, at, width, height)
                .map_err(|e| e.to_string())?;
            // Retain the original unit as well as physical size. Readers that
            // report canvas units also see sheets of equal dimensions.
            doc.update_dict(inserted, |d| {
                d.set(b"UserUnit".to_vec(), pdfcraft_cos::Object::Real(unit));
            })
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(at)
    }
    pub fn redo(&mut self) -> Result<()> {
        self.history(true)
    }
    fn history(&mut self, redo: bool) -> Result<()> {
        let stack = if redo { &self.redo } else { &self.undo };
        let Some(snapshot) = stack.last().cloned() else {
            return Ok(());
        };
        let info =
            pdfcraft_render::inspect(snapshot.bytes.clone(), None).map_err(|e| e.to_string())?;
        let current = self.snapshot();
        if redo {
            self.redo.pop();
            self.undo.push(current);
        } else {
            self.undo.pop();
            self.redo.push(current);
        }
        self.doc = snapshot.doc;
        self.bytes = snapshot.bytes;
        self.revision = snapshot.revision;
        self.info = info;
        Ok(())
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        let task = self.prepare_save(path);
        task.write()?;
        self.accept_save(&task);
        Ok(())
    }
    pub fn prepare_save(&self, path: &Path) -> SaveTask {
        SaveTask {
            instance: self.instance,
            revision: self.revision,
            path: path.to_owned(),
            bytes: self.bytes.clone(),
            expected: self
                .path
                .as_ref()
                .filter(|current| {
                    current.as_path() == path
                        || current
                            .canonicalize()
                            .ok()
                            .zip(path.canonicalize().ok())
                            .is_some_and(|(a, b)| a == b)
                })
                .map(|_| self.saved_bytes.clone()),
        }
    }
    pub fn accept_save(&mut self, task: &SaveTask) {
        if task.instance == self.instance {
            self.recovered_name = None;
            self.canonical_path = task.path.canonicalize().ok();
            self.path = Some(task.path.clone());
            self.saved_revision = task.revision;
            self.saved_bytes = task.bytes.clone();
        }
    }
    pub fn mark_recovered(&mut self) {
        self.revision = self.next_revision;
        self.next_revision += 1;
        self.path = None;
        self.canonical_path = None;
    }
    pub fn blocks(&self, page: usize) -> Result<Vec<TextBlock>> {
        let mut blocks =
            guarded(|| pdfcraft_edit::text_blocks(&self.doc, page).map_err(|e| e.to_string()))?;
        let metadata = pdfcraft_edit::text::formatting_all(&self.doc, page).unwrap_or_default();
        for (index, block) in blocks.iter_mut().enumerate() {
            if let Some(data) = metadata
                .get(&index)
                .and_then(|s| serde_json::from_str::<crate::rich::TextData>(s).ok())
                .filter(crate::rich::TextData::valid)
            {
                let bottom = block.rect[1];
                block.rect = data.rect;
                block.rect[1] = bottom.min(data.rect[1]);
                block.editor_text = data.text.clone();
                block.text = data.text.replace('\n', " ");
            }
        }
        Ok(blocks)
    }
    pub fn text_formatting(&self, page: usize, index: usize) -> Option<crate::rich::TextData> {
        pdfcraft_edit::text::formatting_data(&self.doc, page, index)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(crate::rich::TextData::valid)
    }
    /// Apply a complete character-format transaction, including any user-selected fonts.
    pub fn format_text(
        &mut self,
        page: usize,
        block: Option<usize>,
        added: Option<usize>,
        mut data: crate::rich::TextData,
        fonts: &[(String, Vec<u8>)],
    ) -> Result<usize> {
        if !data.valid() {
            return Err(
                "La mise en forme contient une valeur invalide. Aucun changement appliqué.".into(),
            );
        }
        let mut selected = block.unwrap_or(0);
        self.change(false, |doc| {
            selected = Self::write_formatted_text(doc, page, block, added, &mut data, fonts)?;
            Ok(())
        })?;
        Ok(selected)
    }
    pub(crate) fn write_formatted_text(
        doc: &mut Document,
        page: usize,
        block: Option<usize>,
        added: Option<usize>,
        data: &mut crate::rich::TextData,
        fonts: &[(String, Vec<u8>)],
    ) -> Result<usize> {
        let mut selected = block.unwrap_or(0);
        if block.is_none() {
            let added_index = if let Some(index) = added {
                index
            } else {
                pdfcraft_edit::add_content(
                    doc,
                    page,
                    &Content::Text(AddedText {
                        rect: data.rect,
                        text: "A".into(),
                        size: data.base_size,
                        ..Default::default()
                    }),
                )
                .map_err(|e| e.to_string())?
            };
            let item = pdfcraft_edit::list_added(doc)
                .into_iter()
                .filter(|a| a.page == page)
                .nth(added_index)
                .ok_or("Texte introuvable.")?;
            let object = doc.get(item.obj);
            let pdfcraft_cos::Object::Stream(stream) = object.as_ref() else {
                return Err("Flux de texte invalide.".into());
            };
            let mut dictionary = stream.dict.clone();
            dictionary.remove(b"PCMark");
            dictionary.remove(b"PCAdded");
            let seed = format!("seed-{:?}", item.obj);
            let mut properties = pdfcraft_cos::Dict::new();
            properties.set(
                b"PNText".to_vec(),
                pdfcraft_cos::Object::String(pdfcraft_cos::PdfString::text(&seed)),
            );
            let opening = pdfcraft_content_bytes(&properties);
            let mut content = opening;
            content.extend(stream.decoded().map_err(|e| e.to_string())?);
            content.extend(b"\nEMC\n");
            doc.set(
                item.obj,
                pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(dictionary, &content)),
            );
            selected = (0..pdfcraft_edit::text_blocks(doc, page)
                .map_err(|e| e.to_string())?
                .len())
                .find(|&i| {
                    pdfcraft_edit::text::formatting_data(doc, page, i)
                        .ok()
                        .flatten()
                        .as_deref()
                        == Some(&seed)
                })
                .ok_or("Le nouveau texte n’a pas pu être identifié.")?;
        }
        for (key, bytes) in fonts {
            let resource = Self::embed_user_font(doc, page, bytes)?;
            for span in &mut data.spans {
                if span.format.resource.as_deref() == Some(key) {
                    span.format.resource = Some(resource.clone());
                }
            }
        }
        let source = pdfcraft_edit::text_blocks(doc, page)
            .map_err(|e| e.to_string())?
            .get(selected)
            .cloned()
            .ok_or("Bloc introuvable.")?;
        let previous = pdfcraft_edit::text::formatting_data(doc, page, selected)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<crate::rich::TextData>(&s).ok())
            .filter(crate::rich::TextData::valid);
        let anchor = previous.as_ref().map_or(source.rect, |d| d.rect);
        let leading_empty_rows = previous
            .as_ref()
            .map_or(0, |d| d.text.chars().take_while(|c| *c == '\n').count());
        let original_lines = pdfcraft_edit::text_lines(doc, page).map_err(|e| e.to_string())?;
        let old_size = source
            .lines
            .iter()
            .filter_map(|i| original_lines.get(*i))
            .map(|l| l.size)
            .fold(0.1_f64, f64::max);
        let family = |name: &str| {
            if name.contains("Times") {
                pdfcraft_edit::Family::Times
            } else if name.contains("Courier") {
                pdfcraft_edit::Family::Courier
            } else {
                pdfcraft_edit::Family::Helvetica
            }
        };
        let mut style = BlockStyle {
            width: Some((data.rect[2] - data.rect[0]).max(4.)),
            offset: Some([
                data.rect[0] - anchor[0],
                data.rect[3] - anchor[3]
                    + leading_empty_rows as f64
                        * old_size
                        * previous
                            .as_ref()
                            .and_then(|d| d.line_spacing)
                            .unwrap_or(1.5),
            ]),
            line_spacing: data.line_spacing,
            align: data.align.as_deref().map(|s| match s {
                "Centre" => pdfcraft_edit::Align::Center,
                "Droite" => pdfcraft_edit::Align::Right,
                "Justifié" => pdfcraft_edit::Align::Justify,
                _ => pdfcraft_edit::Align::Left,
            }),
            ..Default::default()
        };
        let mut extras = Vec::new();
        for span in &data.spans {
            let f = &span.format;
            let script = f.script.unwrap_or(0);
            let size = f.size.unwrap_or(data.base_size);
            style.ranges.push(pdfcraft_edit::text::RangeStyle {
                range: span.range.clone(),
                resource: f.resource.clone(),
                family: if f.resource.is_none()
                    && (f.family.is_some() || f.bold.is_some() || f.italic.is_some())
                {
                    Some((
                        family(f.family.as_deref().unwrap_or(&data.font_name)),
                        f.bold.unwrap_or(data.base_bold),
                        f.italic.unwrap_or(data.base_italic),
                    ))
                } else {
                    None
                },
                size: if script != 0 {
                    Some(size * 0.7)
                } else {
                    f.size
                },
                color: f.color,
                underline: f.underline,
                char_spacing: None,
                scale: None,
            });
            extras.push(pdfcraft_edit::text::Decoration {
                range: span.range.clone(),
                strike: f.strike.unwrap_or(false),
                background: if f.highlight.unwrap_or(f.background.is_some()) {
                    f.background.or(Some([1., 0.9, 0.25]))
                } else {
                    None
                },
                rise: f.script.map(|script| match script {
                    1 => size * 0.4,
                    -1 => -size * 0.22,
                    _ => 0.,
                }),
            });
        }
        let encoded = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let result = pdfcraft_edit::text::rewrite_formatted_block(
            doc, page, selected, &data.text, &style, &extras, &encoded,
        )
        .map_err(|e| e.to_string())?;
        if let Some(font) = result.substituted {
            return Err(format!("Police incomplète : {font}"));
        }
        Ok(selected)
    }
    /// A disposable source-free preview. Only the selected drawing is removed;
    /// working bytes, history, signatures and dirty state remain unchanged.
    pub fn preview_without(
        &self,
        page: usize,
        block: Option<usize>,
        added: Option<usize>,
    ) -> Result<Arc<Vec<u8>>> {
        guarded(|| {
            let mut doc = self.doc.clone();
            if let Some(index) = block {
                pdfcraft_edit::clear_block(&mut doc, page, index).map_err(|e| e.to_string())?;
            } else if let Some(index) = added {
                pdfcraft_edit::delete_content(&mut doc, page, index).map_err(|e| e.to_string())?;
            } else {
                return Ok(self.bytes.clone());
            }
            pdfcraft_cos::write_incremental(&doc, &SaveOptions::default())
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn rewrite(
        &mut self,
        page: usize,
        block: usize,
        text: Option<&str>,
        style: &BlockStyle,
    ) -> Result<()> {
        let metadata = self.text_formatting(page, block).or_else(|| {
            style.width?;
            let b = self.blocks(page).ok()?.get(block)?.clone();
            if b.invisible {
                return None;
            }
            Some(crate::rich::TextData {
                id: format!("resized-{page}-{block}"),
                text: b.text,
                rect: b.rect,
                base_size: b.size,
                base_color: b.color,
                base_bold: b.bold,
                base_italic: b.italic,
                font_name: b.base_font,
                align: None,
                line_spacing: None,
                spans: Vec::new(),
            })
        });
        if let Some(mut data) = metadata {
            if let Some(spacing) = style.line_spacing {
                data.line_spacing = Some(spacing);
            }
            if let Some(text) = text {
                crate::rich::remap(&mut data.spans, &data.text, text);
                data.text = text.into();
            }
            if let Some([dx, dy]) = style.offset {
                data.rect = [
                    data.rect[0] + dx,
                    data.rect[1] + dy,
                    data.rect[2] + dx,
                    data.rect[3] + dy,
                ];
            }
            if let Some(width) = style.width {
                data.rect[2] = data.rect[0] + width;
            }
            if let Some(align) = style.align {
                data.align = Some(
                    match align {
                        pdfcraft_edit::Align::Center => "Centre",
                        pdfcraft_edit::Align::Right => "Droite",
                        pdfcraft_edit::Align::Justify => "Justifié",
                        _ => "Gauche",
                    }
                    .into(),
                );
            }
            let delta = crate::rich::Format {
                size: style.size,
                color: style.color,
                underline: style.underline,
                bold: style.bold,
                resource: style.resource.clone(),
                ..Default::default()
            };
            if delta != crate::rich::Format::default() {
                let length = data.text.len();
                crate::rich::apply(&mut data.spans, 0..length, &delta);
            }
            return self
                .format_text(page, Some(block), None, data, &[])
                .map(|_| ());
        }
        let blocks = self.blocks(page)?;
        let b = blocks.get(block).ok_or("Bloc introuvable.")?;
        if b.invisible {
            return Err("Cette couche de texte est invisible (scan/OCR). PDFinator ne recouvre pas l’image. Ajoutez du texte dans une zone libre.".into());
        }
        self.change(false, |d| {
            let result = pdfcraft_edit::rewrite_block(d, page, block, text, style)
                .map_err(|e| format!("Modification refusée, police ou structure non prise en charge : {e}. Vous pouvez choisir explicitement une autre police."))?;
            if let Some(font) = result.substituted {
                return Err(format!("La police {font} ne contient pas tous les caractères demandés. Aucun changement appliqué. Choisissez explicitement une autre police."));
            }
            Ok(())
        })
    }
    /// Remove the original text drawing operators, with one undo transaction.
    pub fn delete_text_block(&mut self, page: usize, block: usize) -> Result<()> {
        self.change(false, |d| {
            pdfcraft_edit::clear_block(d, page, block)
                .map_err(|e| format!("Suppression refusée pour préserver le document : {e}"))
        })
    }
    pub fn add_text(&mut self, page: usize, text: AddedText) -> Result<()> {
        self.change(false, |d| {
            pdfcraft_edit::add_content(d, page, &Content::Text(text))
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }
    /// Move original operators without retyping, preserving TJ spacing and source glyphs.
    pub fn move_block(&mut self, page: usize, block: usize, offset: [f64; 2]) -> Result<()> {
        if self.text_formatting(page, block).is_some() {
            return self.rewrite(
                page,
                block,
                None,
                &BlockStyle {
                    offset: Some(offset),
                    ..Default::default()
                },
            );
        }
        let blocks = self.blocks(page)?;
        let b = blocks.get(block).ok_or("Bloc introuvable.")?;
        if b.invisible {
            return Err(
                "Une couche OCR invisible ne peut pas être déplacée indépendamment du scan.".into(),
            );
        }
        if offset == [0.0, 0.0] {
            return Ok(());
        }
        self.change(false, |d| {
            pdfcraft_edit::objects::move_objects(
                d,
                page,
                &[pdfcraft_edit::ObjectTarget {
                    kind: pdfcraft_edit::ObjectKind::Text,
                    index: block,
                }],
                offset,
            )
            .map_err(|e| format!("Déplacement refusé pour préserver le contenu : {e}"))
        })
    }
    pub fn import_font(&mut self, page: usize, path: &Path) -> Result<String> {
        let bytes = crate::files::read(path)?;
        let mut resource = String::new();
        self.change(false, |d| {
            resource = pdfcraft_edit::text::register_text_font(d, page, &bytes)
                .map_err(|e| format!("Police non incorporable : {e}"))?;
            Ok(())
        })?;
        Ok(resource)
    }
    /// Local fonts are document input. Their embedding rights are checked before writing.
    pub fn embed_user_font(doc: &mut Document, page: usize, bytes: &[u8]) -> Result<String> {
        pdfcraft_fonts::embedding::validate(bytes)?;
        let (_, name) = pdfcraft_fonts::embedding::names(bytes)?;
        let p = pdfcraft_model::pages(doc)
            .get(page)
            .cloned()
            .ok_or("Page introuvable.")?;
        let mut resources = p
            .dict
            .get(b"Resources")
            .and_then(|o| doc.dict(o))
            .unwrap_or_default();
        let mut fonts = resources
            .get(b"Font")
            .and_then(|o| doc.dict(o))
            .unwrap_or_default();
        let key = pdfcraft_fonts::embedding::embed(doc, &mut fonts, bytes, &name)?;
        resources.set(b"Font".to_vec(), pdfcraft_cos::Object::Dict(fonts));
        doc.update_dict(p.obj, |d| {
            d.set(b"Resources".to_vec(), pdfcraft_cos::Object::Dict(resources))
        })
        .map_err(|e| e.to_string())?;
        Ok(key)
    }
    pub fn add_comment(&mut self, page: usize, at: [f64; 2], contents: &str) -> Result<usize> {
        let shape = pdfcraft_annot::Shape::Note {
            at,
            icon: pdfcraft_annot::NoteIcon::Comment,
        };
        let new = pdfcraft_annot::NewAnnotation {
            page,
            style: pdfcraft_annot::Style::default_for(&shape),
            shape,
            contents: contents.into(),
            author: String::new(),
        };
        let mut index = 0;
        self.change(true, |d| {
            index = pdfcraft_annot::add_annotation(d, &new, &Default::default())
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(index)
    }
    pub fn set_comment(&mut self, page: usize, index: usize, text: &str) -> Result<()> {
        self.change(true, |d| {
            pdfcraft_annot::set_contents(d, page, index, text, &Default::default())
                .map_err(|e| e.to_string())
        })
    }
    pub fn added(&self, page: usize) -> Vec<Content> {
        pdfcraft_edit::list_added(&self.doc)
            .into_iter()
            .filter(|a| a.page == page)
            .map(|a| a.content)
            .collect()
    }
    pub fn update_added(&mut self, page: usize, index: usize, content: Content) -> Result<()> {
        self.change(false, |d| {
            pdfcraft_edit::update_content(d, page, index, &content).map_err(|e| e.to_string())
        })
    }
    pub fn delete_added(&mut self, page: usize, index: usize) -> Result<()> {
        self.change(false, |d| {
            pdfcraft_edit::delete_content(d, page, index).map_err(|e| e.to_string())
        })
    }
    pub fn image(
        &mut self,
        page: usize,
        rect: [f64; 4],
        path: &Path,
        replace: Option<usize>,
    ) -> Result<()> {
        let bytes = crate::files::read(path)?;
        self.image_bytes(page, rect, &path.to_string_lossy(), &bytes, replace)
    }
    pub fn image_bytes(
        &mut self,
        page: usize,
        rect: [f64; 4],
        name: &str,
        bytes: &[u8],
        replace: Option<usize>,
    ) -> Result<()> {
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") && !bytes.starts_with(&[255, 216]) {
            return Err("Choisissez une image PNG ou JPEG.".into());
        }
        self.change(false, |d| {
            let (image, _) =
                pdfcraft_create::image_xobject(d, name, bytes).map_err(|e| e.to_string())?;
            let content = Content::Image(AddedImage::new(rect, image));
            if let Some(index) = replace {
                pdfcraft_edit::update_content(d, page, index, &content).map_err(|e| e.to_string())
            } else {
                pdfcraft_edit::add_content(d, page, &content)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        })
    }
    /// Crop one placed image in displayed-page directions, retaining its source
    /// pixels and recording the operation as one undoable transaction.
    pub fn crop_image(
        &mut self,
        page: usize,
        source: crate::ocr::ImageSource,
        margins: [f64; 4],
    ) -> Result<()> {
        if margins
            .iter()
            .any(|v| !v.is_finite() || *v < 0. || *v >= 100.)
            || margins[0] + margins[2] >= 100.
            || margins[1] + margins[3] >= 100.
        {
            return Err("Le rognage doit conserver une partie de l’image.".into());
        }
        if margins == [0.; 4] {
            return Ok(());
        }
        let images = pdfcraft_edit::page_images(&self.doc, page).map_err(|e| e.to_string())?;
        let mut managed = None;
        let index = match source {
            crate::ocr::ImageSource::Original(index) => index,
            crate::ocr::ImageSource::Added(index) => {
                let added = self.added(page);
                let Some(Content::Image(image)) = added.get(index) else {
                    return Err("Image introuvable.".into());
                };
                let objects =
                    pdfcraft_edit::editable_objects(&self.doc, page).map_err(|e| e.to_string())?;
                let expected = objects
                    .iter()
                    .find(|o| {
                        o.target.kind == pdfcraft_edit::ObjectKind::Added && o.target.index == index
                    })
                    .ok_or("Position de l’image introuvable.")?
                    .rect;
                managed = pdfcraft_edit::list_added(&self.doc)
                    .into_iter()
                    .filter(|a| a.page == page)
                    .nth(index)
                    .map(|a| a.obj);
                images
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.object == Some(image.image))
                    .min_by(|(_, a), (_, b)| {
                        let distance = |i: &pdfcraft_edit::PageImage| {
                            i.rect
                                .iter()
                                .zip(expected)
                                .map(|(a, b)| (*a - b).abs())
                                .sum::<f64>()
                        };
                        distance(a).total_cmp(&distance(b))
                    })
                    .map(|(i, _)| i)
                    .ok_or("Image introuvable.")?
            }
        };
        let image = images.get(index).ok_or("Image introuvable.")?;
        let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
        let rect = image.visible_rect.unwrap_or(image.rect);
        let a = geometry.user_to_view(rect[0] as f32, rect[1] as f32);
        let b = geometry.user_to_view(rect[2] as f32, rect[3] as f32);
        let view = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ];
        let w = view[2] - view[0];
        let h = view[3] - view[1];
        let a = geometry.view_to_user(
            view[0] + w * (margins[0] / 100.) as f32,
            view[1] + h * (margins[1] / 100.) as f32,
        );
        let b = geometry.view_to_user(
            view[2] - w * (margins[2] / 100.) as f32,
            view[3] - h * (margins[3] / 100.) as f32,
        );
        let keep = [
            a[0].min(b[0]) as f64,
            a[1].min(b[1]) as f64,
            a[0].max(b[0]) as f64,
            a[1].max(b[1]) as f64,
        ];
        self.change(false, |doc| {
            if let Some(object) = managed {
                doc.update_dict(object, |dict| {
                    dict.remove(b"PCMark");
                    dict.remove(b"PCAdded");
                })
                .map_err(|e| e.to_string())?;
            }
            pdfcraft_edit::change_image(doc, page, index, &pdfcraft_edit::ImageChange::Crop(keep))
                .map_err(|e| e.to_string())
        })
    }
    /// Isolate one raster drawing on a disposable clone for local recognition.
    /// Other page text cannot leak into the recognition result.
    pub fn image_for_ocr(
        &self,
        page: usize,
        source: crate::ocr::ImageSource,
    ) -> Result<(Arc<Vec<u8>>, [f32; 4])> {
        guarded(|| {
            let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
            let images = pdfcraft_edit::page_images(&self.doc, page).map_err(|e| e.to_string())?;
            let image = match source {
                crate::ocr::ImageSource::Original(index) => images.get(index),
                crate::ocr::ImageSource::Added(index) => {
                    let added = self.added(page);
                    let Some(Content::Image(added)) = added.get(index) else {
                        return Err("Image introuvable.".into());
                    };
                    // Use today's actual placement, including later page rotation
                    // and moves, when one image resource appears several times.
                    let expected = pdfcraft_edit::editable_objects(&self.doc, page)
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .find(|o| {
                            o.target.kind == pdfcraft_edit::ObjectKind::Added
                                && o.target.index == index
                        })
                        .map(|o| o.rect)
                        .ok_or("Position de l’image introuvable.")?;
                    images
                        .iter()
                        .filter(|i| i.object == Some(added.image))
                        .min_by(|a, b| {
                            let distance = |i: &pdfcraft_edit::PageImage| {
                                i.rect
                                    .iter()
                                    .zip(expected)
                                    .map(|(a, b)| (*a - b).abs())
                                    .sum::<f64>()
                            };
                            distance(a).total_cmp(&distance(b))
                        })
                }
            }
            .ok_or("Cette image ne peut pas être isolée proprement.")?;
            if image.is_form
                && !image.object.is_some_and(|o| {
                    self.doc.get(o).as_dict().is_some_and(|d| {
                        matches!(
                            d.get(b"PNRasterImage"),
                            Some(pdfcraft_cos::Object::Bool(true))
                        )
                    })
                })
            {
                return Err("Ce bloc est un groupe vectoriel, pas une image à reconnaître.".into());
            }
            let object = image
                .object
                .ok_or("Le flux de cette image n’est pas accessible.")?;
            let rect = image.visible_rect.unwrap_or(image.rect);
            let a = geometry.user_to_view(rect[0] as f32, rect[1] as f32);
            let b = geometry.user_to_view(rect[2] as f32, rect[3] as f32);
            let view = [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[0].max(b[0]),
                a[1].max(b[1]),
            ];
            let mut doc = self.doc.clone();
            let p = pdfcraft_model::pages(&doc)
                .get(page)
                .cloned()
                .ok_or("Page introuvable.")?;
            let mut images = pdfcraft_cos::Dict::new();
            images.set(b"PNOcr".to_vec(), pdfcraft_cos::Object::Ref(object));
            let mut resources = pdfcraft_cos::Dict::new();
            resources.set(b"XObject".to_vec(), pdfcraft_cos::Object::Dict(images));
            let ops = [
                pdfcraft_content::Op::new("q", vec![]),
                pdfcraft_content::Op::new(
                    "cm",
                    image
                        .matrix
                        .into_iter()
                        .map(pdfcraft_cos::Object::Real)
                        .collect(),
                ),
                pdfcraft_content::Op::new("Do", vec![pdfcraft_cos::Object::name("PNOcr")]),
                pdfcraft_content::Op::new("Q", vec![]),
            ];
            let stream = doc.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(
                pdfcraft_cos::Dict::new(),
                &pdfcraft_content::serialize_ops(&ops),
            )));
            doc.update_dict(p.obj, |dict| {
                dict.set(b"Contents".to_vec(), pdfcraft_cos::Object::Ref(stream));
                dict.set(b"Resources".to_vec(), pdfcraft_cos::Object::Dict(resources));
                dict.remove(b"Annots");
            })
            .map_err(|e| e.to_string())?;
            let bytes = pdfcraft_cos::write_incremental(&doc, &SaveOptions::default())
                .map_err(|e| e.to_string())?;
            Ok((Arc::new(bytes), view))
        })
    }
    /// Retain visible PDF labels, and partial invisible layers crossing this image.
    /// Fully contained invisible OCR layers are replaced in the same transaction.
    pub fn image_ocr_retained_text(&self, page: usize, area: [f32; 4]) -> Result<Vec<[f32; 4]>> {
        let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
        Ok(self
            .blocks(page)?
            .into_iter()
            .filter_map(|block| {
                let r = ocr_view_rect(geometry, block.rect);
                (!block.invisible || !ocr_contains(area, r)).then_some(r)
            })
            .collect())
    }
    /// Replace the raster at its original depth, then add editable text above it.
    pub fn image_to_blocks(
        &mut self,
        page: usize,
        source: crate::ocr::ImageSource,
        background: &[u8],
        rect: [f64; 4],
        texts: Vec<AddedText>,
    ) -> Result<Vec<usize>> {
        if texts.is_empty()
            || texts.len() > 4096
            || rect.iter().any(|v| !v.is_finite())
            || rect[2] <= rect[0]
            || rect[3] <= rect[1]
        {
            return Err("Les zones reconnues sont invalides. Aucun changement appliqué.".into());
        }
        for text in &texts {
            if text.text.trim().is_empty()
                || text.text.len() > 1_000_000
                || text.rect.iter().any(|v| !v.is_finite())
                || text.rect[2] <= text.rect[0]
                || text.rect[3] <= text.rect[1]
                || text
                    .color
                    .iter()
                    .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
                || !text.size.is_finite()
                || !(1.0..=500.).contains(&text.size)
            {
                return Err("Une zone de texte contient une valeur invalide.".into());
            }
            if let Some(c) = pdfcraft_fonts::first_undrawable(&text.text) {
                return Err(format!(
                    "Le caractère « {c} » ne peut pas encore être écrit dans cette zone. Corrigez-le dans l’aperçu ; l’image est conservée."
                ));
            }
        }
        let geometry = self
            .info
            .pages
            .get(page)
            .cloned()
            .ok_or("Page introuvable.")?;
        let area = [
            rect[0] as f32,
            geometry.height - rect[3] as f32,
            rect[2] as f32,
            geometry.height - rect[1] as f32,
        ];
        let hidden: Vec<_> = self
            .blocks(page)?
            .into_iter()
            .enumerate()
            .filter(|(_, b)| b.invisible && ocr_contains(area, ocr_view_rect(&geometry, b.rect)))
            .map(|(i, _)| i)
            .collect();
        let mut indices = Vec::new();
        self.change(false, |doc| {
            for index in hidden.iter().rev() {
                pdfcraft_edit::clear_block(doc, page, *index).map_err(|e| e.to_string())?;
            }
            Self::replace_ocr_image(doc, page, source, background, rect, &geometry)?;
            for text in texts {
                indices.push(
                    pdfcraft_edit::add_content(doc, page, &Content::Text(text))
                        .map_err(|e| e.to_string())?,
                );
            }
            Ok(())
        })?;
        Ok(indices)
    }
    fn replace_ocr_image(
        doc: &mut Document,
        page: usize,
        source: crate::ocr::ImageSource,
        background: &[u8],
        rect: [f64; 4],
        geometry: &pdfcraft_render::PageInfo,
    ) -> Result<()> {
        use pdfcraft_cos::{Dict, Object, Stream};
        let images = pdfcraft_edit::page_images(doc, page).map_err(|e| e.to_string())?;
        let mut managed = None;
        let index = match source {
            crate::ocr::ImageSource::Original(index) => index,
            crate::ocr::ImageSource::Added(index) => {
                let item = pdfcraft_edit::list_added(doc)
                    .into_iter()
                    .filter(|a| a.page == page)
                    .nth(index)
                    .ok_or("Image introuvable.")?;
                let Content::Image(added) = item.content else {
                    return Err("L’élément sélectionné n’est pas une image.".into());
                };
                managed = Some(item.obj);
                let expected = pdfcraft_edit::editable_objects(doc, page)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|o| {
                        o.target.kind == pdfcraft_edit::ObjectKind::Added && o.target.index == index
                    })
                    .ok_or("Position de l’image introuvable.")?
                    .rect;
                images
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.object == Some(added.image))
                    .min_by(|(_, a), (_, b)| {
                        let distance = |i: &pdfcraft_edit::PageImage| {
                            i.rect
                                .iter()
                                .zip(expected)
                                .map(|(a, b)| (*a - b).abs())
                                .sum::<f64>()
                        };
                        distance(a).total_cmp(&distance(b))
                    })
                    .map(|(i, _)| i)
                    .ok_or("Image introuvable.")?
            }
        };
        let image = images.get(index).ok_or("Image introuvable.")?;
        let inverse = pdfcraft_content::Matrix(image.matrix)
            .invert()
            .ok_or("Placement d’image invalide.")?;
        let point = |x: f64, y: f64| {
            let p = geometry.view_to_user(x as f32, geometry.height - y as f32);
            inverse.apply(p[0] as f64, p[1] as f64)
        };
        let a = point(rect[0], rect[1]);
        let b = point(rect[2], rect[1]);
        let c = point(rect[0], rect[3]);
        let (raster, _) = pdfcraft_create::image_xobject(doc, "ocr-background.png", background)
            .map_err(|e| e.to_string())?;
        let mut xo = Dict::new();
        xo.set(b"Image".to_vec(), Object::Ref(raster));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        let mut dictionary = Dict::new();
        dictionary.set(b"Type".to_vec(), Object::name("XObject"));
        dictionary.set(b"Subtype".to_vec(), Object::name("Form"));
        dictionary.set(
            b"BBox".to_vec(),
            Object::Array([0., 0., 1., 1.].map(Object::Real).to_vec()),
        );
        dictionary.set(b"Resources".to_vec(), Object::Dict(resources));
        dictionary.set(b"PNRasterImage".to_vec(), Object::Bool(true));
        let replacement = doc.add(Object::Stream(Stream::flate(
            dictionary,
            format!(
                "q {} {} {} {} {} {} cm /Image Do Q\n",
                b.0 - a.0,
                b.1 - a.1,
                c.0 - a.0,
                c.1 - a.1,
                a.0,
                a.1
            )
            .as_bytes(),
        )));
        if let Some(object) = managed {
            doc.update_dict(object, |d| {
                d.remove(b"PCMark");
                d.remove(b"PCAdded");
            })
            .map_err(|e| e.to_string())?;
        }
        // Keep the original drawing order: native labels and their coloured
        // correction patches above this image must remain above the new raster.
        pdfcraft_edit::change_image(
            doc,
            page,
            index,
            &pdfcraft_edit::ImageChange::Replace(replacement),
        )
        .map_err(|e| e.to_string())
    }
    /// Legacy single-paragraph conversion, retained for existing callers.
    pub fn image_to_text(
        &mut self,
        page: usize,
        source: crate::ocr::ImageSource,
        text: AddedText,
    ) -> Result<usize> {
        if text.text.trim().is_empty()
            || text.text.len() > 1_000_000
            || text.rect.iter().any(|v| !v.is_finite())
            || !text.size.is_finite()
            || !(1.0..=500.).contains(&text.size)
        {
            return Err("Le texte reconnu ou ses dimensions ne sont pas valides.".into());
        }
        if let Some(character) = pdfcraft_fonts::first_undrawable(&text.text) {
            return Err(format!(
                "Le caractère « {character} » n’est pas pris en charge par cette conversion. Corrigez-le dans l’aperçu ; l’image est conservée."
            ));
        }
        let mut index = 0;
        self.change(false, |doc| {
            match source {
                crate::ocr::ImageSource::Original(index) => {
                    let images =
                        pdfcraft_edit::page_images(doc, page).map_err(|e| e.to_string())?;
                    if images.get(index).is_none_or(|i| i.is_form) {
                        return Err("L’image à convertir est introuvable.".into());
                    }
                    pdfcraft_edit::change_image(
                        doc,
                        page,
                        index,
                        &pdfcraft_edit::ImageChange::Delete,
                    )
                    .map_err(|e| e.to_string())?;
                }
                crate::ocr::ImageSource::Added(index) => {
                    let item = pdfcraft_edit::list_added(doc)
                        .into_iter()
                        .filter(|a| a.page == page)
                        .nth(index)
                        .ok_or("Image introuvable.")?;
                    if !matches!(item.content, Content::Image(_)) {
                        return Err("L’élément sélectionné n’est pas une image.".into());
                    }
                    pdfcraft_edit::delete_content(doc, page, index).map_err(|e| e.to_string())?;
                }
            }
            index = pdfcraft_edit::add_content(doc, page, &Content::Text(text))
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(index)
    }
    pub fn text(&self, page: usize) -> Result<Arc<PageText>> {
        let mut renderer = PageRenderer::new(self.bytes.clone(), RenderConfig::default());
        let output = renderer.render(RenderRequest {
            page,
            kind: RequestKind::Text,
            ..Default::default()
        });
        if let Some(e) = output.error {
            return Err(e);
        }
        output.text.ok_or("Aucune couche de texte.".into())
    }
    pub fn highlight(
        &mut self,
        page: usize,
        text: &PageText,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        let geometry = self.info.pages.get(page).ok_or("Page introuvable.")?;
        if range.is_empty() || range.end > text.glyphs.len() {
            return Err("Sélection invalide.".into());
        }
        let quads = text
            .line_rects(range.clone())
            .into_iter()
            .map(|r| geometry.view_rect_to_quad(r))
            .collect();
        let shape = pdfcraft_annot::Shape::TextMarkup {
            kind: pdfcraft_annot::Markup::Highlight,
            quads,
        };
        let new = pdfcraft_annot::NewAnnotation {
            page,
            style: pdfcraft_annot::Style::default_for(&shape),
            shape,
            contents: text.text_of(range),
            author: String::new(),
        };
        self.change(true, |d| {
            pdfcraft_annot::add_annotation(d, &new, &pdfcraft_annot::Meta::default())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }
    pub fn delete_highlight(&mut self, page: usize, index: usize) -> Result<()> {
        self.change(true, |d| {
            pdfcraft_annot::delete_annotation(d, page, index).map_err(|e| e.to_string())
        })
    }
    /// Repainting already highlighted letters erases those letters only.
    pub fn toggle_highlight(
        &mut self,
        page: usize,
        text: &PageText,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        let geometry = self
            .info
            .pages
            .get(page)
            .ok_or("Page introuvable.")?
            .clone();
        if range.is_empty() || range.end > text.glyphs.len() {
            return Err("Sélection invalide.".into());
        }
        let annotations: Vec<_> = pdfcraft_annot::summaries(&self.doc)
            .into_iter()
            .filter(|a| a.page == page && a.subtype == "Highlight" && !a.locked)
            .map(|a| {
                let covered: Vec<_> = text
                    .glyphs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, g)| {
                        let center = [(g.rect[0] + g.rect[2]) / 2., (g.rect[1] + g.rect[3]) / 2.];
                        a.quads
                            .iter()
                            .any(|q| {
                                let pts: Vec<_> = [0, 2, 6, 4]
                                    .into_iter()
                                    .map(|j| geometry.user_to_view(q[j], q[j + 1]))
                                    .collect();
                                let crosses: Vec<_> = (0..4)
                                    .map(|j| {
                                        let p = pts[j];
                                        let n = pts[(j + 1) % 4];
                                        (n[0] - p[0]) * (center[1] - p[1])
                                            - (n[1] - p[1]) * (center[0] - p[0])
                                    })
                                    .collect();
                                crosses.iter().all(|v| *v >= -0.01)
                                    || crosses.iter().all(|v| *v <= 0.01)
                            })
                            .then_some(i)
                    })
                    .collect();
                (a, covered)
            })
            .collect();
        if !range
            .clone()
            .filter(|&i| !text.glyphs[i].text.trim().is_empty())
            .all(|i| annotations.iter().any(|(_, glyphs)| glyphs.contains(&i)))
        {
            return self.highlight(page, text, range);
        }
        let mut changes: Vec<_> = annotations
            .into_iter()
            .filter(|(_, glyphs)| glyphs.iter().any(|i| range.contains(i)))
            .collect();
        changes.sort_by_key(|(a, _)| std::cmp::Reverse(a.index));
        self.change(true, |doc| {
            for (a, glyphs) in changes {
                let remaining: Vec<_> = glyphs.into_iter().filter(|i| !range.contains(i)).collect();
                let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
                for i in remaining {
                    if let Some(last) = runs.last_mut().filter(|r| r.end == i) {
                        last.end += 1;
                    } else {
                        runs.push(i..i + 1);
                    }
                }
                pdfcraft_annot::delete_annotation(doc, page, a.index).map_err(|e| e.to_string())?;
                if !runs.is_empty() {
                    let shape = pdfcraft_annot::Shape::TextMarkup {
                        kind: pdfcraft_annot::Markup::Highlight,
                        quads: runs
                            .into_iter()
                            .flat_map(|r| text.line_rects(r))
                            .map(|r| geometry.view_rect_to_quad(r))
                            .collect(),
                    };
                    let mut style = pdfcraft_annot::Style::default_for(&shape);
                    if let Some(color) = a.color {
                        style.color = color.map(f64::from);
                    }
                    pdfcraft_annot::add_annotation(
                        doc,
                        &pdfcraft_annot::NewAnnotation {
                            page,
                            shape,
                            style,
                            contents: a.contents.unwrap_or_default(),
                            author: a.author.unwrap_or_default(),
                        },
                        &pdfcraft_annot::Meta::default(),
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })
    }
    pub fn color_highlight(&mut self, page: usize, index: usize, color: [f64; 3]) -> Result<()> {
        self.change(true, |d| {
            pdfcraft_annot::set_style(
                d,
                page,
                index,
                Some(color),
                None,
                None,
                None,
                &pdfcraft_annot::Meta::default(),
            )
            .map_err(|e| e.to_string())
        })
    }
}

fn next_instance() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
#[derive(Clone)]
pub struct SaveTask {
    pub instance: u64,
    revision: u64,
    pub path: PathBuf,
    pub bytes: Arc<Vec<u8>>,
    expected: Option<Arc<Vec<u8>>>,
}
impl SaveTask {
    pub fn write(&self) -> Result<()> {
        guarded(|| {
            Document::open(self.bytes.clone()).map_err(|e| e.to_string())?;
            if self.path.exists()
                && let Some(expected) = &self.expected
                && crate::files::read(&self.path)?.as_slice() != expected.as_slice()
            {
                return Err("Le fichier a été modifié ailleurs. Utilisez Enregistrer sous.".into());
            }
            crate::files::atomic_write(&self.path, &self.bytes)
        })
    }
}
