#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod icons;
mod ui;

fn main() -> eframe::Result {
    let path = std::env::args_os()
        .skip(1)
        .find(|a| !a.to_string_lossy().starts_with("--"))
        .map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_maximized(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/pdfinator.png"))
                    .unwrap_or_default(),
            )
            .with_inner_size([1080.0, 680.0])
            .with_min_inner_size([760.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PDFinator — Ergonatus",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, path)))),
    )
}
