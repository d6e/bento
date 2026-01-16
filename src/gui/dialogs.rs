use eframe::egui;
use std::path::PathBuf;

/// State for the config file chooser dialog
pub struct ConfigChooserDialog {
    pub bento_files: Vec<PathBuf>,
    pub selected_index: usize,
}

impl ConfigChooserDialog {
    pub fn new(bento_files: Vec<PathBuf>) -> Self {
        Self {
            bento_files,
            selected_index: 0,
        }
    }

    /// Returns Some(path) if user selected a file, None if still choosing
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut result = None;

        egui::Window::new("Choose Config File")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Multiple .bento files found. Select one:");
                ui.add_space(8.0);

                for (i, path) in self.bento_files.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());

                    if ui
                        .selectable_label(self.selected_index == i, &name)
                        .clicked()
                    {
                        self.selected_index = i;
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        result = Some(self.bento_files[self.selected_index].clone());
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(PathBuf::new()); // Empty path = cancelled
                    }
                });
            });

        result
    }
}

/// Find all .bento files in a directory
pub fn find_bento_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "bento") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
