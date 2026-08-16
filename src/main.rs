mod app;
mod data;
mod emoji_cache;
mod gif_provider;
mod media_clipboard;
mod media_drag;
mod preflight;
mod settings;
mod ui;

use app::SymbolisApp;
use eframe::egui::ViewportBuilder;

const APP_NAME: &str = "Symbolis";

fn main() -> eframe::Result {
    let preflight = match preflight::run_startup_preflight() {
        Ok(report) => report,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_min_inner_size([420.0, 360.0])
            .with_resizable(true)
            .with_title(APP_NAME),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(SymbolisApp::new(cc, preflight)))),
    )
}
