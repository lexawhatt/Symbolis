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
use eframe::egui::{self, Align, CentralPanel, Layout, RichText, ViewportBuilder};

const APP_NAME: &str = "Symbolis";

fn main() -> eframe::Result {
    let preflight = match preflight::run_startup_preflight() {
        Ok(report) => report,
        Err(err) => {
            let message = err.to_string();
            eprintln!("{message}");
            return run_startup_error_window(message);
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

fn run_startup_error_window(message: String) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([560.0, 360.0])
            .with_min_inner_size([420.0, 280.0])
            .with_resizable(true)
            .with_title("Symbolis startup check"),
        ..Default::default()
    };

    eframe::run_native(
        "Symbolis startup check",
        options,
        Box::new(|_| Ok(Box::new(StartupErrorApp { message }))),
    )
}

struct StartupErrorApp {
    message: String,
}

impl eframe::App for StartupErrorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(18.0);
            ui.heading(RichText::new("Symbolis cannot start").size(22.0));
            ui.add_space(12.0);
            ui.label(&self.message);

            ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}
