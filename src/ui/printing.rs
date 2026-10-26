use super::{App, Color32, Icon, Vec2, egui, icons};
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};

pub enum Event {
    Printers(Vec<craft_print::Printer>),
    Finished(Result<String, String>),
}
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Pages {
    #[default]
    All,
    Current,
    Custom,
}
pub struct Printing {
    pub pages: Pages,
    pub range: String,
    pub open: bool,
    pub printers: Vec<craft_print::Printer>,
    pub selected: usize,
    pub copies: u32,
    pub grayscale: bool,
    pub duplex: craft_print::Duplex,
    pub scaling: craft_print::Scaling,
    pub busy: bool,
    pub failed: bool,
    pub message: String,
    rx: Option<Receiver<Event>>,
}
impl Default for Printing {
    fn default() -> Self {
        Self {
            pages: Pages::All,
            range: String::new(),
            open: false,
            printers: Vec::new(),
            selected: 0,
            copies: 1,
            grayscale: false,
            duplex: craft_print::Duplex::Off,
            scaling: craft_print::Scaling::ShrinkToFit,
            busy: false,
            failed: false,
            message: String::new(),
            rx: None,
        }
    }
}
impl Printing {
    pub fn load(&mut self) {
        self.open = true;
        if self.busy {
            return;
        }
        self.busy = true;
        self.failed = false;
        self.message = "Recherche des imprimantes…".into();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let event = std::panic::catch_unwind(craft_print::printers)
                .map(Event::Printers)
                .unwrap_or_else(|_| {
                    Event::Finished(Err("La recherche des imprimantes s’est interrompue.".into()))
                });
            let _ = tx.send(event);
        });
    }
    pub fn poll(&mut self) {
        let event = match self.rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(event)) => Some(event),
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.rx = None;
                self.busy = false;
                self.failed = true;
                self.message = "L’opération d’impression s’est interrompue.".into();
                None
            }
            _ => None,
        };
        if let Some(event) = event {
            self.rx = None;
            self.busy = false;
            match event {
                Event::Printers(list) => {
                    self.selected = list.iter().position(|p| p.default).unwrap_or(0);
                    self.message = if list.is_empty() {
                        "Aucune imprimante Windows disponible.".into()
                    } else {
                        String::new()
                    };
                    self.printers = list;
                    if self
                        .printers
                        .get(self.selected)
                        .is_some_and(|p| p.two_sided == Some(false))
                    {
                        self.duplex = craft_print::Duplex::Off;
                    }
                }
                Event::Finished(result) => {
                    self.failed = result.is_err();
                    self.message = match result {
                        Ok(job) => format!("Document transmis à l’imprimante : {job}"),
                        Err(e) => format!("L’impression a échoué : {e}"),
                    }
                }
            }
        }
    }
    fn job(&self, title: String) -> Result<craft_print::Job, String> {
        if self.busy {
            return Err("Une opération d’impression est déjà en cours.".into());
        }
        let printer = self
            .printers
            .get(self.selected)
            .ok_or("Choisissez une imprimante disponible.")?;
        if self.duplex != craft_print::Duplex::Off && printer.two_sided == Some(false) {
            return Err(
                "Cette imprimante ne prend pas en charge le recto verso automatique.".into(),
            );
        }
        Ok(craft_print::Job {
            printer: Some(printer.name.clone()),
            copies: self.copies.clamp(1, 99),
            grayscale: self.grayscale,
            duplex: Some(self.duplex),
            scaling: self.scaling,
            title,
            ..Default::default()
        })
    }
    /// Invoked only by the person's explicit Print action, never by validation/tests.
    pub fn submit(&mut self, bytes: Arc<Vec<u8>>, title: String, pages: Option<Vec<usize>>) {
        let job = match self.job(title) {
            Ok(job) => job,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.busy = true;
        self.message = "Envoi à l’imprimante…".into();
        self.failed = false;
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let bytes = match pages {
                    Some(pages) => Arc::new(
                        pdfinator::print_pages::subset(bytes, &pages)
                            .map_err(craft_print::Error::Spool)?,
                    ),
                    None => bytes,
                };
                craft_print::submit(&bytes, &job)
            }))
            .map_err(|_| "Le service d’impression n’a pas pu traiter le document.".to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(Event::Finished(result));
        });
    }
}

impl App {
    pub(super) fn print_dialog(&mut self, ctx: &egui::Context) {
        let count = self.editor.as_ref().map_or(0, |e| e.info.pages.len());
        let name = self.document_label(self.active_document);
        let mut send = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("print-dialog"))
            .backdrop_color(Color32::from_black_alpha(85))
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(249, 251, 255))
                    .corner_radius(16.)
                    .inner_margin(20.)
                    .stroke(egui::Stroke::new(1., Color32::from_rgb(214, 225, 243)))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 12],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_black_alpha(55),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 96.).clamp(320., 460.));
                ui.spacing_mut().item_spacing = Vec2::new(10., 6.);
                ui.spacing_mut().interact_size.y = 32.;
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.), egui::Sense::hover());
                    icons::draw(ui.painter(), Icon::Print, rect, icons::BLUE);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Imprimer")
                                .strong()
                                .size(23.)
                                .color(icons::INK),
                        );
                        ui.add(egui::Label::new(egui::RichText::new(&name).size(12.)).truncate())
                            .on_hover_text(&name);
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("print-dialog-settings")
                    .auto_shrink([false, true])
                    .max_height((ctx.content_rect().height() - 190.).max(120.))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!self.printing.busy, |ui| {
                            ui.strong("Imprimante");
                            let selected = self
                                .printing
                                .printers
                                .get(self.printing.selected)
                                .map_or("Aucune imprimante disponible", |p| p.name.as_str());
                            let old = self.printing.selected;
                            egui::ComboBox::from_id_salt("print-printer")
                                .width(ui.available_width())
                                .wrap_mode(egui::TextWrapMode::Truncate)
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (i, printer) in self.printing.printers.iter().enumerate() {
                                        let label = if printer.default {
                                            format!("{} · par défaut", printer.name)
                                        } else {
                                            printer.name.clone()
                                        };
                                        if ui
                                            .add(
                                                egui::Button::selectable(
                                                    self.printing.selected == i,
                                                    &label,
                                                )
                                                .wrap_mode(egui::TextWrapMode::Wrap),
                                            )
                                            .on_hover_text(&printer.name)
                                            .clicked()
                                        {
                                            self.printing.selected = i;
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(selected);
                            if old != self.printing.selected
                                && self
                                    .printing
                                    .printers
                                    .get(self.printing.selected)
                                    .is_some_and(|p| p.two_sided == Some(false))
                            {
                                self.printing.duplex = craft_print::Duplex::Off;
                            }
                            ui.add_space(3.);
                            egui::Grid::new("print-settings")
                                .num_columns(2)
                                .spacing(Vec2::new(22., 10.))
                                .show(ui, |ui| {
                                    ui.label("Pages");
                                    ui.vertical(|ui| {
                                        egui::ComboBox::from_id_salt("print-pages")
                                            .selected_text(match self.printing.pages {
                                                Pages::All => "Toutes les pages",
                                                Pages::Current => "Page actuelle",
                                                Pages::Custom => "Plage personnalisée",
                                            })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut self.printing.pages,
                                                    Pages::All,
                                                    format!("Toutes les pages · {count}"),
                                                );
                                                ui.selectable_value(
                                                    &mut self.printing.pages,
                                                    Pages::Current,
                                                    format!("Page actuelle · {}", self.page + 1),
                                                );
                                                ui.selectable_value(
                                                    &mut self.printing.pages,
                                                    Pages::Custom,
                                                    "Plage personnalisée",
                                                );
                                            });
                                        if self.printing.pages == Pages::Custom {
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut self.printing.range,
                                                )
                                                .hint_text("Ex. : 1-3, 5, 8")
                                                .desired_width(230.),
                                            );
                                            if let Err(error) = pdfinator::print_pages::parse(
                                                &self.printing.range,
                                                count,
                                            ) {
                                                ui.colored_label(
                                                    Color32::from_rgb(180, 35, 60),
                                                    error,
                                                );
                                            }
                                        }
                                    });
                                    ui.end_row();
                                    ui.label("Exemplaires");
                                    ui.add(
                                        egui::DragValue::new(&mut self.printing.copies)
                                            .range(1..=99),
                                    )
                                    .on_hover_text("Nombre d’exemplaires");
                                    ui.end_row();
                                    ui.label("Couleur");
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(
                                            &mut self.printing.grayscale,
                                            false,
                                            "Couleur",
                                        );
                                        ui.selectable_value(
                                            &mut self.printing.grayscale,
                                            true,
                                            "Noir et blanc",
                                        );
                                    });
                                    ui.end_row();
                                    ui.label("Impression");
                                    let duplex_available = self
                                        .printing
                                        .printers
                                        .get(self.printing.selected)
                                        .is_none_or(|p| p.two_sided != Some(false));
                                    egui::ComboBox::from_id_salt("print-duplex")
                                        .width(280.)
                                        .selected_text(match self.printing.duplex {
                                            craft_print::Duplex::Off => "Recto uniquement",
                                            craft_print::Duplex::LongEdge => {
                                                "Recto verso · bord long"
                                            }
                                            craft_print::Duplex::ShortEdge => {
                                                "Recto verso · bord court"
                                            }
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut self.printing.duplex,
                                                craft_print::Duplex::Off,
                                                "Recto uniquement",
                                            );
                                            ui.add_enabled_ui(duplex_available, |ui| {
                                                ui.selectable_value(
                                                    &mut self.printing.duplex,
                                                    craft_print::Duplex::LongEdge,
                                                    "Recto verso · bord long",
                                                );
                                                ui.selectable_value(
                                                    &mut self.printing.duplex,
                                                    craft_print::Duplex::ShortEdge,
                                                    "Recto verso · bord court",
                                                );
                                            });
                                        });
                                    ui.end_row();
                                    ui.label("Taille");
                                    egui::ComboBox::from_id_salt("print-scaling")
                                        .width(280.)
                                        .selected_text(match self.printing.scaling {
                                            craft_print::Scaling::ShrinkToFit => {
                                                "Adapter à la zone imprimable"
                                            }
                                            craft_print::Scaling::Exact => "Taille réelle · 100 %",
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut self.printing.scaling,
                                                craft_print::Scaling::ShrinkToFit,
                                                "Adapter à la zone imprimable",
                                            );
                                            ui.selectable_value(
                                                &mut self.printing.scaling,
                                                craft_print::Scaling::Exact,
                                                "Taille réelle · 100 %",
                                            );
                                        });
                                    ui.end_row();
                                });
                        });
                        ui.add_space(2.);
                        if !self.printing.message.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                if self.printing.busy {
                                    ui.spinner();
                                }
                                ui.add(egui::Label::new(&self.printing.message).wrap());
                            });
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(
                                        "Vos modifications sont incluses dans l’impression.",
                                    )
                                    .size(12.)
                                    .color(Color32::from_rgb(87, 107, 139)),
                                )
                                .wrap(),
                            );
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    close = ui
                        .add_sized([110., 36.], egui::Button::new("Fermer").corner_radius(8.))
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        send = ui
                            .add_enabled(
                                !self.printing.busy
                                    && !self.printing.printers.is_empty()
                                    && count > 0
                                    && (self.printing.pages != Pages::Custom
                                        || pdfinator::print_pages::parse(
                                            &self.printing.range,
                                            count,
                                        )
                                        .is_ok()),
                                // The selected range is validated again before submission.
                                egui::Button::new(
                                    egui::RichText::new("Imprimer")
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(icons::BLUE)
                                .corner_radius(8.)
                                .min_size(Vec2::new(138., 36.)),
                            )
                            .clicked();
                    });
                });
            });
        if send {
            if let Some(e) = &self.editor {
                let pages = match self.printing.pages {
                    Pages::All => Ok(None),
                    Pages::Current => Ok(Some(vec![self.page])),
                    Pages::Custom => {
                        pdfinator::print_pages::parse(&self.printing.range, count).map(Some)
                    }
                };
                match pages {
                    Ok(pages) => self.printing.submit(e.bytes.clone(), name, pages),
                    Err(error) => self.printing.message = error,
                }
            }
        }
        if close || modal.should_close() {
            self.printing.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printer_results_are_consumed_once_and_a_disconnected_worker_cannot_stay_busy() {
        let mut p = Printing::default();
        let (tx, rx) = mpsc::channel();
        p.rx = Some(rx);
        p.busy = true;
        tx.send(Event::Printers(vec![craft_print::Printer {
            name: "Test".into(),
            default: true,
            two_sided: Some(false),
        }]))
        .unwrap();
        p.poll();
        assert!(!p.busy);
        assert_eq!(p.printers.len(), 1);
        let (tx, rx) = mpsc::channel();
        p.rx = Some(rx);
        p.busy = true;
        drop(tx);
        p.poll();
        assert!(!p.busy);
        assert!(p.message.contains("interrompue"));
    }
    #[test]
    fn settings_reach_the_print_job_and_unsupported_duplex_is_refused() {
        let mut printing = Printing {
            printers: vec![craft_print::Printer {
                name: "HP Smart Tank 7000".into(),
                default: true,
                two_sided: Some(true),
            }],
            copies: 3,
            grayscale: true,
            duplex: craft_print::Duplex::ShortEdge,
            scaling: craft_print::Scaling::Exact,
            ..Default::default()
        };
        let job = printing.job("Cours corrigé.pdf".into()).unwrap();
        assert_eq!(job.printer.as_deref(), Some("HP Smart Tank 7000"));
        assert_eq!(job.copies, 3);
        assert!(job.grayscale);
        assert_eq!(job.duplex, Some(craft_print::Duplex::ShortEdge));
        assert_eq!(job.scaling, craft_print::Scaling::Exact);
        assert_eq!(job.title, "Cours corrigé.pdf");
        printing.printers[0].two_sided = Some(false);
        assert!(
            printing
                .job("Test".into())
                .unwrap_err()
                .contains("recto verso")
        );
        printing.duplex = craft_print::Duplex::Off;
        assert!(printing.job("Test".into()).is_ok());
        printing.busy = true;
        assert!(printing.job("Test".into()).is_err());
        printing.busy = false;
        printing.printers.clear();
        assert!(printing.job("Test".into()).is_err());
    }
}
