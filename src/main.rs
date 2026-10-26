#![windows_subsystem = "windows"]

mod app;
mod doc;
mod i18n;
mod search;
mod store;
mod theme;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let settings = store::load_settings();
    let size = settings.window_size.unwrap_or([1200.0, 820.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MVCreader")
            .with_inner_size(size)
            .with_min_inner_size([640.0, 420.0])
            .with_icon(app::icon())
            .with_drag_and_drop(true),
        ..Default::default()
    };
    // Archivo pasado por línea de comandos ("Abrir con…" desde el Explorador).
    let file = std::env::args_os().nth(1).map(PathBuf::from);
    eframe::run_native(
        "MVCreader",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, settings, file)))),
    )
}
