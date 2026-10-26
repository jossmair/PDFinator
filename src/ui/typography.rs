//! Fonts are local document inputs; no system font is copied into the application.
use super::*;
use std::sync::OnceLock;
#[derive(Clone)]
pub(super) struct FontFace {
    pub name: String,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub path: Option<PathBuf>,
    pub bundled: Option<&'static [u8]>,
}
impl FontFace {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        match (&self.path, self.bundled) {
            (Some(p), _) => pdfinator::files::read(p),
            (_, Some(b)) => Ok(b.to_vec()),
            _ => Err("Police indisponible.".into()),
        }
    }
}
pub(super) struct FontBook {
    pub entries: Arc<Vec<FontFace>>,
    pub query: String,
    pending: Option<std::sync::mpsc::Receiver<Arc<Vec<FontFace>>>>,
}
impl FontBook {
    pub fn start() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(Self::discover().entries);
        });
        Self {
            query: String::new(),
            pending: Some(receiver),
            entries: Arc::new(vec![FontFace {
                name: "Inter".into(),
                family: "Inter".into(),
                bold: false,
                italic: false,
                path: None,
                bundled: Some(include_bytes!(
                    "../../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf"
                )),
            }]),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(entries) => {
                    self.entries = entries;
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
    }
    pub fn discover() -> Self {
        static FONTS: OnceLock<Arc<Vec<FontFace>>> = OnceLock::new();
        Self {
            query: String::new(),
            pending: None,
            entries: FONTS
                .get_or_init(|| {
                    let mut entries = Vec::new();
                    for (name, bold, bytes) in [
                        (
                            "Inter",
                            false,
                            include_bytes!("../../vendor/pdfcraft/assets/fonts/Inter-Regular.ttf")
                                .as_slice(),
                        ),
                        (
                            "Inter Semi Bold",
                            true,
                            include_bytes!("../../vendor/pdfcraft/assets/fonts/Inter-SemiBold.ttf")
                                .as_slice(),
                        ),
                    ] {
                        entries.push(FontFace {
                            name: name.into(),
                            family: "Inter".into(),
                            bold,
                            italic: false,
                            path: None,
                            bundled: Some(bytes),
                        });
                    }
                    let mut directories = Vec::new();
                    if let Some(windows) = std::env::var_os("WINDIR") {
                        directories.push(PathBuf::from(windows).join("Fonts"));
                    }
                    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                        directories.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
                    }
                    for dir in directories {
                        let Ok(files) = std::fs::read_dir(dir) else {
                            continue;
                        };
                        for file in files.flatten().take(4096) {
                            let path = file.path();
                            if !path.extension().is_some_and(|s| {
                                s.eq_ignore_ascii_case("ttf") || s.eq_ignore_ascii_case("otf")
                            }) {
                                continue;
                            }
                            if file.metadata().is_ok_and(|m| m.len() > 32_000_000) {
                                continue;
                            }
                            let Ok(bytes) = pdfinator::files::read(&path) else {
                                continue;
                            };
                            if pdfcraft_fonts::embedding::validate(&bytes).is_err() {
                                continue;
                            }
                            let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else {
                                continue;
                            };
                            let Ok((family, postscript)) = pdfcraft_fonts::embedding::names(&bytes)
                            else {
                                continue;
                            };
                            let declarations: String = face
                                .names()
                                .into_iter()
                                .filter(|n| matches!(n.name_id, 0 | 8))
                                .filter_map(|n| n.to_string())
                                .collect();
                            let key = family.to_ascii_lowercase().replace(' ', "");
                            if declarations.to_ascii_lowercase().contains("adobe")
                                || [
                                    "sourcehan",
                                    "sourcesans",
                                    "sourceserif",
                                    "notosanscjk",
                                    "notoserifcjk",
                                ]
                                .iter()
                                .any(|s| key.starts_with(s))
                            {
                                continue;
                            }
                            let name = face
                                .names()
                                .into_iter()
                                .filter(|n| n.name_id == 4)
                                .find_map(|n| n.to_string())
                                .unwrap_or(postscript);
                            entries.push(FontFace {
                                name,
                                family,
                                bold: face.is_bold(),
                                italic: face.is_italic(),
                                path: Some(path),
                                bundled: None,
                            });
                        }
                    }
                    entries.sort_by_key(|a| a.name.to_lowercase());
                    entries.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
                    Arc::new(entries)
                })
                .clone(),
        }
    }
    pub fn covering(&self, preferred: &str, text: &str) -> Option<FontFace> {
        let normalize = |s: &str| {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let key = normalize(preferred.rsplit('+').next().unwrap_or(preferred));
        let alias = if key.contains("helvetica") {
            "arial"
        } else if key.starts_with("times") {
            "timesnewroman"
        } else if key.contains("courier") {
            "couriernew"
        } else {
            &key
        };
        let mut candidates: Vec<_> = self.entries.iter().collect();
        candidates.sort_by_key(|f| {
            let family = normalize(&f.family);
            if family == alias || key.starts_with(&family) {
                0
            } else if f.family == "Inter" && !f.bold {
                1
            } else {
                2
            }
        });
        candidates
            .into_iter()
            .find(|f| {
                f.bytes()
                    .is_ok_and(|b| pdfcraft_fonts::embedding::covers(&b, text))
            })
            .cloned()
    }
}

impl Selection {
    pub(super) fn active_format(&self) -> pdfinator::rich::Format {
        let index = self.text_range.as_ref().map_or(0, |r| r.start);
        self.rich
            .iter()
            .find(|s| s.range.contains(&index))
            .map(|s| s.format.clone())
            .unwrap_or_else(|| self.typing.clone())
    }
    fn data(&self, p: &PageInfo) -> pdfinator::rich::TextData {
        let rect = if matches!(self.target, Target::New | Target::Added(_)) {
            let v = self.view_rect(p);
            p.view_rect_to_user([v.left(), v.top(), v.right(), v.bottom()])
                .map(f64::from)
        } else {
            self.rect
        };
        pdfinator::rich::TextData {
            id: format!("pn-{}-{:?}", self.page, self.target),
            text: self.draft.clone(),
            rect,
            base_size: self.size,
            base_color: self.color.map(f64::from),
            base_bold: self.bold,
            base_italic: self.italic,
            font_name: self.font_name.clone(),
            align: self.align.map(|a| {
                match a {
                    Align::Center => "Centre",
                    Align::Right => "Droite",
                    Align::Justify => "Justifié",
                    _ => "Gauche",
                }
                .into()
            }),
            spans: self.rich.clone(),
            line_spacing: self.line_spacing,
        }
    }
}
impl App {
    pub(super) fn apply_format(
        &mut self,
        ctx: &egui::Context,
        delta: pdfinator::rich::Format,
        font: Option<FontFace>,
    ) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        if self.inline.as_ref().is_none_or(|i| i.committed) {
            self.begin_text(ctx, selected);
        }
        let Some(inline) = &mut self.inline else {
            return;
        };
        if let Some(font) = font {
            let Some(key) = delta.resource.clone() else {
                return;
            };
            let bytes = match font.bytes() {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            };
            let name = format!("chosen-{}", font.name);
            let mut definitions = ctx.fonts(|f| f.definitions().clone());
            definitions
                .font_data
                .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            let fallback = definitions
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
                .unwrap_or_default();
            let mut stack = vec![name.clone()];
            stack.extend(fallback);
            definitions
                .families
                .insert(egui::FontFamily::Name(name.clone().into()), stack);
            ctx.set_fonts(definitions);
            inline.fonts.insert(key.clone(), name);
            inline.selection.font_bindings.retain(|(k, _)| k != &key);
            inline.selection.font_bindings.push((key, font));
        }
        let s = &mut inline.selection;
        if s.draft.is_empty() {
            s.typing.overlay(&delta);
        }
        let range = if s.scope_all {
            0..s.draft.len()
        } else {
            s.text_range.clone().unwrap_or(0..s.draft.len())
        };
        pdfinator::rich::apply(&mut s.rich, range, &delta);
        self.selected = Some(inline.selection.clone());
        // Buttons and palette choices return to the existing caret. A numeric
        // size field keeps its own focus until the user has finished entering it.
        if delta.size.is_none() && !egui::Popup::is_any_open(ctx) {
            inline.focus = true;
        }
    }
    pub(super) fn choose_font(&mut self, ctx: &egui::Context, font: FontFace) {
        let key = format!("chosen:{}", font.name);
        self.apply_format(
            ctx,
            pdfinator::rich::Format {
                resource: Some(key),
                font_name: Some(font.name.clone()),
                bold: Some(font.bold),
                italic: Some(font.italic),
                ..Default::default()
            },
            Some(font),
        );
    }
    pub(super) fn font_variant(&mut self, ctx: &egui::Context, bold: bool, italic: bool) {
        let Some(s) = self
            .inline
            .as_ref()
            .map(|i| &i.selection)
            .or(self.selected.as_ref())
        else {
            return;
        };
        let name = s
            .active_format()
            .font_name
            .unwrap_or_else(|| s.font_name.clone());
        let base = self
            .fonts
            .entries
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.family.clone())
            .unwrap_or(name.clone());
        let key = pdfcraft_fonts::embedding::face_key(&base);
        let alias = if key.contains("helvetica") {
            "arial"
        } else if key.contains("times") {
            "timesnewroman"
        } else if key.contains("courier") {
            "couriernew"
        } else {
            &key
        };
        let font = self
            .fonts
            .entries
            .iter()
            .find(|f| {
                pdfcraft_fonts::embedding::face_key(&f.family) == alias
                    && f.bold == bold
                    && f.italic == italic
            })
            .cloned();
        if let Some(font) = font {
            self.choose_font(ctx, font);
        } else {
            self.apply_format(
                ctx,
                pdfinator::rich::Format {
                    bold: Some(bold),
                    italic: Some(italic),
                    ..Default::default()
                },
                None,
            );
        }
    }
    pub(super) fn text_controls(&mut self, ui: &mut egui::Ui, s: &Selection) {
        let ctx = ui.ctx().clone();
        let format = s.active_format();
        ui.label("Police");
        let name = format.font_name.as_deref().unwrap_or(&s.font_name);
        let mut choose = None;
        egui::ComboBox::from_id_salt("font-names")
            .width(208.)
            .height(300.)
            .selected_text(friendly_font_name(name))
            .show_ui(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.fonts.query)
                        .hint_text("Rechercher une police…")
                        .desired_width(205.),
                );
                let query = self.fonts.query.to_lowercase();
                for font in self
                    .fonts
                    .entries
                    .iter()
                    .filter(|f| f.name.to_lowercase().contains(&query))
                {
                    if ui.selectable_label(font.name == name, &font.name).clicked() {
                        choose = Some(font.clone());
                        ui.close();
                    }
                }
            });
        if let Some(font) = choose {
            self.choose_font(&ctx, font);
        }
        ui.add_space(8.);
        let mut size = format.size.unwrap_or(s.size);
        let mut changed = false;
        let mut finished = false;
        ui.horizontal(|ui| {
            ui.label("Taille");
            (changed, finished) = font_size_picker(ui, &mut size);
        });
        if changed {
            self.apply_format(
                &ctx,
                pdfinator::rich::Format {
                    size: Some(size),
                    ..Default::default()
                },
                None,
            );
        }
        if finished && let Some(inline) = &mut self.inline {
            inline.focus = true;
        }
        ui.horizontal(|ui| {
            let bold = format.bold.unwrap_or(s.bold);
            let italic = format.italic.unwrap_or(s.italic);
            if icons::chip(ui, Icon::Bold, "", bold)
                .on_hover_text("Gras · Ctrl+B")
                .clicked()
            {
                self.font_variant(&ctx, !bold, italic);
            }
            if icons::chip(ui, Icon::Italic, "", italic)
                .on_hover_text("Italique · Ctrl+I")
                .clicked()
            {
                self.font_variant(&ctx, bold, !italic);
            }
            if icons::chip(ui, Icon::Underline, "", format.underline.unwrap_or(false))
                .on_hover_text("Souligner · Ctrl+U")
                .clicked()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        underline: Some(!format.underline.unwrap_or(false)),
                        ..Default::default()
                    },
                    None,
                );
            }
            if icons::chip(ui, Icon::Strike, "", format.strike.unwrap_or(false))
                .on_hover_text("Barrer")
                .clicked()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        strike: Some(!format.strike.unwrap_or(false)),
                        ..Default::default()
                    },
                    None,
                );
            }
            if icons::chip(ui, Icon::Superscript, "", format.script == Some(1))
                .on_hover_text("Exposant · x²")
                .clicked()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        script: Some(if format.script == Some(1) { 0 } else { 1 }),
                        ..Default::default()
                    },
                    None,
                );
            }
            if icons::chip(ui, Icon::Subscript, "", format.script == Some(-1))
                .on_hover_text("Indice · H₂O")
                .clicked()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        script: Some(if format.script == Some(-1) { 0 } else { -1 }),
                        ..Default::default()
                    },
                    None,
                );
            }
        });
        ui.add_space(8.);
        ui.label("Couleur du texte");
        let mut color = format
            .color
            .unwrap_or(s.color.map(f64::from))
            .map(|x| x as f32);
        if self.palette(ui, &mut color) {
            self.apply_format(
                &ctx,
                pdfinator::rich::Format {
                    color: Some(color.map(f64::from)),
                    ..Default::default()
                },
                None,
            );
        }
        ui.horizontal(|ui| {
            if icons::chip(
                ui,
                Icon::Highlight,
                "",
                format.highlight.unwrap_or(format.background.is_some()),
            )
            .on_hover_text("Surligner les caractères sélectionnés")
            .clicked()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        highlight: Some(!format.highlight.unwrap_or(format.background.is_some())),
                        ..Default::default()
                    },
                    None,
                );
            }
            let mut highlight = format
                .background
                .unwrap_or([1., 0.9, 0.25])
                .map(|x| x as f32);
            if ui
                .color_edit_button_rgb(&mut highlight)
                .on_hover_text("Couleur du surlignage")
                .changed()
            {
                self.apply_format(
                    &ctx,
                    pdfinator::rich::Format {
                        highlight: Some(true),
                        background: Some(highlight.map(f64::from)),
                        ..Default::default()
                    },
                    None,
                );
            }
        });
        ui.add_space(8.);
        ui.label("Alignement du bloc");
        ui.horizontal(|ui| {
            for (align, icon) in [
                (Align::Left, Icon::AlignLeft),
                (Align::Center, Icon::AlignCenter),
                (Align::Right, Icon::AlignRight),
                (Align::Justify, Icon::AlignJustify),
            ] {
                if icons::chip(ui, icon, "", s.align == Some(align))
                    .on_hover_text(icon.label())
                    .clicked()
                {
                    if self.inline.as_ref().is_none_or(|i| i.committed) {
                        self.begin_text(&ctx, s.clone());
                    }
                    if let Some(i) = &mut self.inline {
                        i.selection.align = Some(align);
                        i.focus = true;
                        self.selected = Some(i.selection.clone());
                    }
                }
            }
        });
        ui.add_space(8.);
        let bullets = pdfinator::rich::bulleted(&s.draft, s.text_range.clone());
        if icons::chip(ui, Icon::Bullets, "Puces", bullets).clicked() {
            if self.inline.as_ref().is_none_or(|i| i.committed) {
                self.begin_text(&ctx, s.clone());
            }
            if let Some(inline) = &mut self.inline {
                inline.toggle_bullets();
                self.selected = Some(inline.selection.clone());
            }
        }
        ui.add_space(8.);
        ui.horizontal(|ui| {
            ui.label("Interligne");
            let mut spacing = s.line_spacing;
            let (changed, finished) = line_spacing_picker(ui, &mut spacing);
            if changed {
                if self.inline.as_ref().is_none_or(|i| i.committed) {
                    self.begin_text(&ctx, s.clone());
                }
                if let Some(inline) = &mut self.inline {
                    inline.selection.line_spacing = spacing;
                    self.selected = Some(inline.selection.clone());
                }
            }
            if finished && let Some(inline) = &mut self.inline {
                inline.focus = true;
            }
        });
    }
    pub(super) fn palette(&self, ui: &mut egui::Ui, color: &mut [f32; 3]) -> bool {
        let mut changed = false;
        egui::Grid::new("text-palette")
            .min_col_width(0.)
            .spacing(Vec2::splat(5.))
            .show(ui, |ui| {
                for (index, (name, rgb)) in [
                    ("Noir", [0.04, 0.06, 0.1]),
                    ("Gris", [0.38, 0.42, 0.48]),
                    ("Bleu", [0.13, 0.33, 0.95]),
                    ("Violet", [0.51, 0.22, 0.83]),
                    ("Rose", [0.9, 0.1, 0.38]),
                    ("Rouge", [0.8, 0.06, 0.04]),
                    ("Orange", [0.95, 0.4, 0.08]),
                    ("Vert", [0.02, 0.52, 0.34]),
                    ("Turquoise", [0.0, 0.55, 0.63]),
                    ("Brun", [0.48, 0.27, 0.14]),
                    ("Jaune", [0.95, 0.72, 0.1]),
                    ("Blanc", [1., 1., 1.]),
                ]
                .into_iter()
                .enumerate()
                {
                    let (r, a) = ui.allocate_exact_size(Vec2::splat(27.), egui::Sense::click());
                    a.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name)
                    });
                    ui.painter().rect_filled(
                        r.shrink(2.),
                        7.,
                        Color32::from_rgb(
                            (rgb[0] * 255.) as u8,
                            (rgb[1] * 255.) as u8,
                            (rgb[2] * 255.) as u8,
                        ),
                    );
                    if *color == rgb {
                        ui.painter().rect_stroke(
                            r,
                            8.,
                            Stroke::new(2., icons::BLUE),
                            StrokeKind::Outside,
                        );
                    }
                    if a.on_hover_text(name).clicked() {
                        *color = rgb;
                        changed = true;
                    }
                    if index % 6 == 5 {
                        ui.end_row();
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.label("Personnalisée");
            changed |= ui.color_edit_button_rgb(color).changed();
        });
        changed
    }
    pub(super) fn commit_text_selection(&mut self, s: &Selection) -> Result<Target> {
        let p = self
            .editor
            .as_ref()
            .and_then(|e| e.info.pages.get(s.page))
            .cloned()
            .ok_or("Page introuvable.")?;
        let mut data = s.data(&p);
        if data.spans.is_empty() && s.typing != Default::default() {
            pdfinator::rich::apply(&mut data.spans, 0..data.text.len(), &s.typing);
        }
        let fonts: Result<Vec<_>> = s
            .font_bindings
            .iter()
            .map(|(key, f)| f.bytes().map(|b| (key.clone(), b)))
            .collect();
        let mut fonts = fonts?;
        let rich = !data.spans.is_empty()
            || data.line_spacing.is_some()
            || s.align != s.original_align
            || self
                .editor
                .as_ref()
                .and_then(|e| {
                    if let Target::Block(i) = s.target {
                        e.text_formatting(s.page, i)
                    } else {
                        None
                    }
                })
                .is_some();
        let Some(e) = &mut self.editor else {
            return Err("Aucun document.".into());
        };
        let mut target = s.target;
        let apply =
            |e: &mut Editor, data: pdfinator::rich::TextData, fonts: &[(String, Vec<u8>)]| {
                e.format_text(
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
                    data,
                    fonts,
                )
                .map(Target::Block)
            };
        let result = if rich {
            apply(e, data.clone(), &fonts)
        } else {
            let text = AddedText {
                rect: s.rect,
                text: s.draft.clone(),
                size: s.size,
                color: s.color.map(f64::from),
                bold: s.bold,
                italic: s.italic,
                family: s.family.unwrap_or(Family::Helvetica),
                align: s.align.unwrap_or(Align::Left),
            };
            match s.target {
                Target::New => {
                    target = Target::Added(e.added(s.page).len());
                    e.add_text(s.page, text).map(|_| target)
                }
                Target::Added(i) => e
                    .update_added(s.page, i, Content::Text(text))
                    .map(|_| target),
                Target::Block(i) => e
                    .rewrite(s.page, i, Some(&s.draft), &s.style())
                    .map(|_| target),
                _ => Ok(target),
            }
        };
        match result {
            Ok(target) => Ok(target),
            Err(error)
                if error.to_lowercase().contains("police")
                    || error.to_lowercase().contains("font")
                    || error.to_lowercase().contains("glyph") =>
            {
                let font = self
                    .fonts
                    .covering(&s.font_name, &s.draft)
                    .ok_or(error.clone())?;
                let key = "automatic-full-face".to_string();
                fonts.push((key.clone(), font.bytes()?));
                pdfinator::rich::apply(
                    &mut data.spans,
                    0..data.text.len(),
                    &pdfinator::rich::Format {
                        resource: Some(key),
                        font_name: Some(font.name.clone()),
                        ..Default::default()
                    },
                );
                data.font_name = font.name;
                apply(e, data, &fonts)
            }
            Err(error) => Err(error),
        }
    }
}

/// Presets close the menu and return to the text caret. Custom input keeps its
/// own focus until Enter, so typing a value never redirects digits into the PDF.
fn font_size_picker(ui: &mut egui::Ui, size: &mut f64) -> (bool, bool) {
    let label = |value: f64| {
        let value = format!("{value:.2}");
        format!("{} pt", value.trim_end_matches('0').trim_end_matches('.')).replace('.', ",")
    };
    let mut changed = false;
    let mut finished = false;
    let response = egui::ComboBox::from_id_salt("font-sizes")
        .width(106.)
        .height(280.)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(label(*size))
        .show_ui(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Autre");
                let custom = ui.add(
                    egui::DragValue::new(size)
                        .range(4.0..=200.)
                        .clamp_existing_to_range(false)
                        .speed(0.5)
                        .max_decimals(2)
                        .suffix(" pt")
                        .custom_parser(|value| {
                            value
                                .trim()
                                .replace(',', ".")
                                .parse::<f64>()
                                .ok()
                                .filter(|size| size.is_finite())
                        }),
                );
                custom.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        true,
                        "Autre taille de police",
                    )
                });
                changed |= custom.changed();
                if custom.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    finished = true;
                    ui.close();
                }
            });
            ui.separator();
            for preset in [
                6., 8., 9., 10., 10.5, 11., 12., 14., 16., 18., 20., 22., 24., 26., 28., 32., 36.,
                48., 60., 72., 96., 120., 144., 200.,
            ] {
                if ui
                    .selectable_label((*size - preset).abs() < 0.001, label(preset))
                    .clicked()
                {
                    *size = preset;
                    changed = true;
                    finished = true;
                    ui.close();
                }
            }
        });
    response.response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(
            egui::WidgetType::ComboBox,
            true,
            "Choisir une taille de police",
        );
        info.current_text_value = Some(label(*size));
        info
    });
    (changed, finished)
}

fn line_spacing_picker(ui: &mut egui::Ui, spacing: &mut Option<f64>) -> (bool, bool) {
    let label = |v: f64| format!("{v}").replace('.', ",");
    let mut changed = false;
    let mut finished = false;
    let response = egui::ComboBox::from_id_salt("line-spacing")
        .width(106.)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(spacing.map_or_else(|| "D’origine".into(), label))
        .show_ui(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Autre");
                let mut value = spacing.unwrap_or(1.5);
                let custom = ui.add(
                    egui::DragValue::new(&mut value)
                        .range(0.5..=5.)
                        .speed(0.05)
                        .custom_parser(|v| {
                            v.trim()
                                .replace(',', ".")
                                .parse::<f64>()
                                .ok()
                                .filter(|v| v.is_finite())
                        }),
                );
                custom.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, "Autre interligne")
                });
                if custom.changed() {
                    *spacing = Some(value);
                    changed = true;
                }
                if custom.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    finished = true;
                    ui.close();
                }
            });
            ui.separator();
            for value in [1., 1.15, 1.5, 2., 2.5, 3.] {
                if ui
                    .selectable_label(*spacing == Some(value), format!("{} ×", label(value)))
                    .clicked()
                {
                    *spacing = Some(value);
                    changed = true;
                    finished = true;
                    ui.close();
                }
            }
        });
    response.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Choisir un interligne")
    });
    (changed, finished)
}

fn friendly_font_name(name: &str) -> String {
    let raw = name.split_once('+').map_or(name, |(_, n)| n);
    let family = if raw.contains("TimesNewRoman") {
        "Times New Roman"
    } else if raw.starts_with("Arial") {
        "Arial"
    } else {
        return raw.to_string();
    };
    let style = match (raw.contains("Bold"), raw.contains("Italic")) {
        (true, true) => " — Gras italique",
        (true, false) => " — Gras",
        (false, true) => " — Italique",
        _ => "",
    };
    format!("{family}{style}")
}
