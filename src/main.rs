mod app;
mod auth;
mod gemini;
mod types;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Gemini Client")
            .with_inner_size([960.0, 680.0])
            .with_min_inner_size([640.0, 440.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Gemini Client",
        native_options,
        Box::new(|cc| Ok(Box::new(app::GeminiApp::new(cc)))),
    )
}
