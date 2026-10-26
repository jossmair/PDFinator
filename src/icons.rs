//! Original PDFinator duotone SVG pictograms, rendered at the actual display density.
use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2};
#[derive(Clone, Copy, Debug)]
pub enum Icon {
    Brand,
    Open,
    Save,
    Print,
    Select,
    Hand,
    Copy,
    Paste,
    AddPdf,
    BlankPage,
    Move,
    Edit,
    Text,
    Image,
    Drawing,
    Line,
    Circle,
    Square,
    Rectangle,
    Table,
    Up,
    Down,
    Highlight,
    Undo,
    Redo,
    Search,
    Previous,
    Next,
    Plus,
    Minus,
    Width,
    Page,
    Delete,
    Close,
    Apply,
    Replace,
    Export,
    Minimize,
    Maximize,
    RotateLeft,
    RotateRight,
    Bold,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Comment,
    Italic,
    Underline,
    Strike,
    Superscript,
    Subscript,
    AlignJustify,
    Bullets,
    Word,
}
pub const INK: Color32 = Color32::from_rgb(30, 49, 73);
pub const BLUE: Color32 = Color32::from_rgb(55, 113, 249);
pub const RAIL: Color32 = Color32::from_rgb(16, 25, 42);
impl Icon {
    pub fn label(self) -> &'static str {
        match self {
            Self::Brand => "PDFinator",
            Self::Move => "Déplacer",
            Self::Open => "Ouvrir",
            Self::Save => "Enregistrer",
            Self::Print => "Imprimer",
            Self::Select => "Sélectionner",
            Self::Hand => "Main · déplacer la vue",
            Self::Copy => "Copier le bloc",
            Self::Paste => "Coller le bloc",
            Self::AddPdf => "Ajouter des PDF",
            Self::BlankPage => "Ajouter une page blanche",
            Self::Edit => "Modifier le texte",
            Self::Text => "Ajouter du texte",
            Self::Image => "Insérer une image",
            Self::Drawing => "Dessin",
            Self::Line => "Trait",
            Self::Circle => "Cercle",
            Self::Square => "Carré",
            Self::Rectangle => "Rectangle",
            Self::Table => "Tableau",
            Self::Up => "Haut",
            Self::Down => "Bas",
            Self::Highlight => "Surligner",
            Self::Undo => "Annuler",
            Self::Redo => "Rétablir",
            Self::Search => "Rechercher",
            Self::Previous => "Précédent",
            Self::Next => "Suivant",
            Self::Plus => "Augmenter le zoom",
            Self::Minus => "Réduire le zoom",
            Self::Width => "Adapter à la largeur",
            Self::Page => "Page entière",
            Self::Delete => "Supprimer",
            Self::Close => "Fermer",
            Self::Apply => "Valider",
            Self::Replace => "Remplacer",
            Self::Export => "Enregistrer sous",
            Self::Minimize => "Réduire la fenêtre",
            Self::Maximize => "Agrandir la fenêtre",
            Self::RotateLeft => "Tourner à gauche",
            Self::RotateRight => "Tourner à droite",
            Self::Bold => "Gras",
            Self::AlignLeft => "Aligner à gauche",
            Self::AlignCenter => "Centrer",
            Self::AlignRight => "Aligner à droite",
            Self::Comment => "Commentaire",
            Self::Italic => "Italique",
            Self::Underline => "Souligner",
            Self::Strike => "Barrer",
            Self::Superscript => "Exposant",
            Self::Subscript => "Indice",
            Self::AlignJustify => "Justifier",
            Self::Bullets => "Puces",
            Self::Word => "Exporter en DOCX ou ODT",
        }
    }
    fn source(self) -> egui::ImageSource<'static> {
        match self {
            Self::Brand => egui::include_image!("../assets/pdfinator.svg"),
            Self::Move => egui::include_image!("../assets/icons/move.svg"),
            Self::Open => egui::include_image!("../assets/icons/open.svg"),
            Self::Save => egui::include_image!("../assets/icons/save.svg"),
            Self::Print => egui::include_image!("../assets/icons/print.svg"),
            Self::Select => egui::include_image!("../assets/icons/select.svg"),
            Self::Hand => egui::include_image!("../assets/icons/hand.svg"),
            Self::Copy => egui::include_image!("../assets/icons/copy.svg"),
            Self::Paste => egui::include_image!("../assets/icons/paste.svg"),
            Self::AddPdf => egui::include_image!("../assets/icons/add-pdf.svg"),
            Self::BlankPage => egui::include_image!("../assets/icons/blank-page.svg"),
            Self::Edit => egui::include_image!("../assets/icons/edit.svg"),
            Self::Text => egui::include_image!("../assets/icons/text.svg"),
            Self::Image => egui::include_image!("../assets/icons/image.svg"),
            Self::Drawing => egui::include_image!("../assets/icons/drawing.svg"),
            Self::Line => egui::include_image!("../assets/icons/line.svg"),
            Self::Circle => egui::include_image!("../assets/icons/circle.svg"),
            Self::Square => egui::include_image!("../assets/icons/square.svg"),
            Self::Rectangle => egui::include_image!("../assets/icons/rectangle.svg"),
            Self::Table => egui::include_image!("../assets/icons/table.svg"),
            Self::Up => egui::include_image!("../assets/icons/up.svg"),
            Self::Down => egui::include_image!("../assets/icons/down.svg"),

            Self::Highlight => egui::include_image!("../assets/icons/highlight.svg"),
            Self::Undo => egui::include_image!("../assets/icons/undo.svg"),
            Self::Redo => egui::include_image!("../assets/icons/redo.svg"),
            Self::Search => egui::include_image!("../assets/icons/search.svg"),
            Self::Previous => egui::include_image!("../assets/icons/previous.svg"),
            Self::Next => egui::include_image!("../assets/icons/next.svg"),
            Self::Plus => egui::include_image!("../assets/icons/plus.svg"),
            Self::Minus => egui::include_image!("../assets/icons/minus.svg"),
            Self::Width => egui::include_image!("../assets/icons/width.svg"),
            Self::Page => egui::include_image!("../assets/icons/page.svg"),
            Self::Delete => egui::include_image!("../assets/icons/delete.svg"),
            Self::Close => egui::include_image!("../assets/icons/close.svg"),
            Self::Apply => egui::include_image!("../assets/icons/apply.svg"),
            Self::Replace => egui::include_image!("../assets/icons/replace.svg"),
            Self::Export => egui::include_image!("../assets/icons/export.svg"),
            Self::Minimize => egui::include_image!("../assets/icons/minimize.svg"),
            Self::Maximize => egui::include_image!("../assets/icons/maximize.svg"),
            Self::RotateLeft => egui::include_image!("../assets/icons/rotateleft.svg"),
            Self::RotateRight => egui::include_image!("../assets/icons/rotateright.svg"),
            Self::Bold => egui::include_image!("../assets/icons/bold.svg"),
            Self::AlignLeft => egui::include_image!("../assets/icons/alignleft.svg"),
            Self::AlignCenter => egui::include_image!("../assets/icons/aligncenter.svg"),
            Self::AlignRight => egui::include_image!("../assets/icons/alignright.svg"),
            Self::Comment => egui::include_image!("../assets/icons/comment.svg"),
            Self::Italic => egui::include_image!("../assets/icons/italic.svg"),
            Self::Underline => egui::include_image!("../assets/icons/underline.svg"),
            Self::Strike => egui::include_image!("../assets/icons/strike.svg"),
            Self::Superscript => egui::include_image!("../assets/icons/superscript.svg"),
            Self::Subscript => egui::include_image!("../assets/icons/subscript.svg"),
            Self::AlignJustify => egui::include_image!("../assets/icons/alignjustify.svg"),
            Self::Bullets => egui::include_image!("../assets/icons/bullets.svg"),
            Self::Word => egui::include_image!("../assets/icons/word.svg"),
        }
    }
}
pub fn draw(p: &egui::Painter, icon: Icon, r: Rect, color: Color32) {
    let image = egui::Image::new(icon.source()).fit_to_exact_size(r.size());
    if let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(p.ctx(), r.size()) {
        let tint = if color == Color32::from_gray(170) {
            Color32::from_white_alpha(70)
        } else {
            Color32::WHITE
        };
        p.image(
            texture.id,
            r,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            tint,
        );
    }
}
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
    enabled: bool,
    size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let motion = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::new("animations")))
        .unwrap_or(true);
    let h = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        enabled && response.hovered(),
        if motion { 0.14 } else { 0. },
    );
    let face = rect.shrink(2.).translate(Vec2::new(0., -h));
    if active {
        ui.painter()
            .rect_filled(face.expand(2.), 14., Color32::from_rgb(25, 46, 78));
        ui.painter()
            .rect_filled(face, 12., Color32::from_rgb(30, 62, 105));
        ui.painter().rect_stroke(
            face,
            12.,
            Stroke::new(1., Color32::from_rgb(68, 119, 194)),
            StrokeKind::Inside,
        );
    } else if h > 0. {
        ui.painter().rect_filled(
            face,
            12.,
            Color32::from_rgba_unmultiplied(90, 125, 178, (h * 40.) as u8),
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            face.expand(1.),
            12.,
            Stroke::new(2., Color32::from_rgb(141, 208, 255)),
            StrokeKind::Inside,
        );
    }
    draw(
        ui.painter(),
        icon,
        Rect::from_center_size(face.center(), Vec2::splat((size - 10.).clamp(21., 28.) + h)),
        if enabled {
            INK
        } else {
            Color32::from_gray(170)
        },
    );
    response
}
pub fn chip(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> Response {
    let enabled = ui.is_enabled();
    let text_width = if label.is_empty() {
        0.
    } else {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(label.into(), egui::FontId::proportional(12.), INK)
                .size()
                .x
                + 8.
        })
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30. + text_width, 32.), Sense::click());
    let accessible = if label.is_empty() {
        icon.label()
    } else {
        label
    };
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible));
    if enabled && (active || response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(
            rect,
            8.,
            if active {
                Color32::from_rgb(219, 231, 255)
            } else {
                Color32::from_rgba_unmultiplied(120, 151, 200, 28)
            },
        );
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 8., Stroke::new(1., BLUE), StrokeKind::Inside);
    }
    draw(
        ui.painter(),
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 15., rect.center().y),
            Vec2::splat(23.),
        ),
        if enabled {
            INK
        } else {
            Color32::from_gray(170)
        },
    );
    if !label.is_empty() {
        ui.painter().text(
            Pos2::new(rect.left() + 30., rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.),
            INK,
        );
    }
    response
}
pub fn small(ui: &mut egui::Ui, icon: Icon, label: &str) -> Response {
    chip(ui, icon, label, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_svg_pictogram_renders_at_high_resolution() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        for icon in [
            Icon::Brand,
            Icon::Move,
            Icon::Open,
            Icon::Save,
            Icon::Print,
            Icon::Select,
            Icon::Hand,
            Icon::Copy,
            Icon::Paste,
            Icon::AddPdf,
            Icon::BlankPage,
            Icon::Edit,
            Icon::Text,
            Icon::Image,
            Icon::Drawing,
            Icon::Line,
            Icon::Circle,
            Icon::Square,
            Icon::Rectangle,
            Icon::Table,
            Icon::Up,
            Icon::Down,
            Icon::Highlight,
            Icon::Undo,
            Icon::Redo,
            Icon::Search,
            Icon::Previous,
            Icon::Next,
            Icon::Plus,
            Icon::Minus,
            Icon::Width,
            Icon::Page,
            Icon::Delete,
            Icon::Close,
            Icon::Apply,
            Icon::Replace,
            Icon::Export,
            Icon::Minimize,
            Icon::Maximize,
            Icon::RotateLeft,
            Icon::RotateRight,
            Icon::Bold,
            Icon::AlignLeft,
            Icon::AlignCenter,
            Icon::AlignRight,
            Icon::Comment,
            Icon::Italic,
            Icon::Underline,
            Icon::Strike,
            Icon::Superscript,
            Icon::Subscript,
            Icon::AlignJustify,
            Icon::Bullets,
            Icon::Word,
        ] {
            let loaded = egui::Image::new(icon.source())
                .fit_to_exact_size(Vec2::splat(96.))
                .load_for_size(&ctx, Vec2::splat(96.));
            let Ok(egui::load::TexturePoll::Ready { texture }) = loaded else {
                panic!("SVG could not render: {icon:?}")
            };
            // SizedTexture.size is the SVG's intrinsic 32pt size. Verify the
            // uploaded pixel buffer, which must use the requested resolution.
            let manager = ctx.tex_manager();
            assert_eq!(manager.read().meta(texture.id).unwrap().size, [96, 96]);
        }
    }
}
