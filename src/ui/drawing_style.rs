//! Compact visual stroke selectors, shared with the live drawing preview.
use super::*;
use pdfinator::drawing::StrokeStyle;

pub(super) fn stroke_path(
    painter: &egui::Painter,
    points: &[Pos2],
    width: f32,
    color: Color32,
    style: &StrokeStyle,
    unit_scale: f64,
) {
    if points.len() < 2 {
        return;
    }
    if *style == StrokeStyle::Double {
        let direction = (points[1] - points[0]).normalized();
        let normal = Vec2::new(-direction.y, direction.x) * width / 3.;
        for offset in [-normal, normal] {
            painter.line_segment(
                [points[0] + offset, points[1] + offset],
                Stroke::new(width / 3., color),
            );
        }
        return;
    }
    let (mut pattern, mut phase, cap) = style.pattern(width as f64);
    if matches!(style, StrokeStyle::Custom { .. }) {
        for value in &mut pattern {
            *value *= unit_scale;
        }
        phase *= unit_scale;
    }
    if pattern.is_empty() {
        painter.add(egui::Shape::line(
            points.to_vec(),
            Stroke::new(width, color),
        ));
        return;
    }
    if pattern.len() % 2 == 1 {
        pattern.extend(pattern.clone());
    }
    let period = pattern.iter().sum::<f64>();
    if !period.is_finite() || period <= 0. {
        return;
    }
    phase = phase.rem_euclid(period);
    let mut index = 0;
    while phase > 0. && phase >= pattern[index] {
        phase -= pattern[index];
        index = (index + 1) % pattern.len();
    }
    let mut remaining = (pattern[index] - phase) as f32;
    let mut budget = 20_000;
    for pair in points.windows(2) {
        let delta = pair[1] - pair[0];
        let length = delta.length();
        if length <= 0. {
            continue;
        }
        let direction = delta / length;
        let mut at = 0.;
        while at < length && budget > 0 {
            budget -= 1;
            if remaining <= 0.00001 {
                if index % 2 == 0 && cap == 1 {
                    painter.circle_filled(pair[0] + direction * at, width / 2., color);
                }
                index = (index + 1) % pattern.len();
                remaining = pattern[index] as f32;
                continue;
            }
            let step = remaining.min(length - at);
            if index % 2 == 0 {
                painter.line_segment(
                    [pair[0] + direction * at, pair[0] + direction * (at + step)],
                    Stroke::new(width, color),
                );
            }
            at += step;
            remaining -= step;
        }
    }
}

fn choice(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    width: f32,
    style: &StrokeStyle,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(198.), 29.),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if active || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, 5., Color32::from_rgb(222, 233, 255));
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 5., Stroke::new(1., icons::BLUE), StrokeKind::Inside);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(7., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.),
        icons::INK,
    );
    stroke_path(
        ui.painter(),
        &[
            Pos2::new(rect.right() - 76., rect.center().y),
            Pos2::new(rect.right() - 9., rect.center().y),
        ],
        width,
        icons::INK,
        style,
        1.,
    );
    response
}

fn point_label(value: f64) -> String {
    format!("{value} pt").replace('.', ",")
}

pub(super) fn width_picker(ui: &mut egui::Ui, width: &mut f64) -> bool {
    let mut changed = false;
    let response = egui::ComboBox::from_id_salt("drawing-width")
        .width(ui.available_width() - 8.)
        .height(280.)
        .selected_text(point_label(*width))
        .show_ui(ui, |ui| {
            for preset in [0.25, 0.5, 0.75, 1., 1.5, 2.25, 3., 4.5, 6., 8., 12.] {
                if choice(
                    ui,
                    &point_label(preset),
                    (*width - preset).abs() < 0.001,
                    preset as f32,
                    &StrokeStyle::Solid,
                )
                .clicked()
                {
                    *width = preset;
                    changed = true;
                    ui.close();
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Autre");
                changed |= ui
                    .add(
                        egui::DragValue::new(width)
                            .range(0.1..=100.)
                            .speed(0.1)
                            .suffix(" pt"),
                    )
                    .changed();
            });
        });
    response.response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Choisir une épaisseur");
        info.current_text_value = Some(point_label(*width));
        info
    });
    changed
}

pub(super) fn style_picker(ui: &mut egui::Ui, style: &mut StrokeStyle, double: bool) -> bool {
    let mut changed = false;
    let response = egui::ComboBox::from_id_salt("drawing-stroke-style")
        .width(ui.available_width() - 8.)
        .height(240.)
        .selected_text(style.label())
        .show_ui(ui, |ui| {
            for preset in [
                StrokeStyle::Solid,
                StrokeStyle::Dashed,
                StrokeStyle::Dotted,
                StrokeStyle::DashDot,
                StrokeStyle::LongDash,
                StrokeStyle::Double,
            ] {
                if preset == StrokeStyle::Double && !double {
                    continue;
                }
                if choice(ui, preset.label(), *style == preset, 2.4, &preset).clicked() {
                    *style = preset;
                    changed = true;
                    ui.close();
                }
            }
        });
    response.response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Choisir un type de trait");
        info.current_text_value = Some(style.label().into());
        info
    });
    changed
}
