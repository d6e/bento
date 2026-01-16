mod app;
mod dialogs;
mod panels;
pub mod state;
mod thumbnail;

use anyhow::Result;
use eframe::egui;

/// Check if a path has a supported image extension
pub(crate) fn is_supported_image(path: &std::path::Path) -> bool {
    const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

pub fn run(initial_path: Option<std::path::PathBuf>) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Bento",
        options,
        Box::new(move |cc| Ok(Box::new(app::BentoApp::new(cc, initial_path)))),
    )
    .map_err(|e| anyhow::anyhow!("Failed to run GUI: {}", e))
}
