use anyhow::Result;
use eframe::egui;

struct BentoApp;

impl BentoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self
    }
}

impl eframe::App for BentoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |_ui| {
            // Empty for now
        });
    }
}

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bento - Sprite Atlas Packer",
        options,
        Box::new(|cc| Ok(Box::new(BentoApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("Failed to run GUI: {}", e))
}
