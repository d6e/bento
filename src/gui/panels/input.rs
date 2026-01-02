use eframe::egui;

use crate::gui::state::{AppState, OutputFormat};
use crate::gui::is_supported_image;

/// Input panel with file list, output path, and format selection
pub fn input_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Input Sprites");

    ui.add_space(4.0);

    // File action buttons
    ui.horizontal(|ui| {
        if ui.button("+ Add Files").clicked() {
            let mut dialog = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]);
            if let Some(dir) = &state.runtime.last_input_dir {
                dialog = dialog.set_directory(dir);
            }
            if let Some(paths) = dialog.pick_files() {
                // Remember the directory of the first file
                if let Some(first) = paths.first() {
                    state.runtime.last_input_dir = first.parent().map(|p| p.to_path_buf());
                }
                state.config.input_paths.extend(paths);
            }
        }

        if ui.button("+ Add Folder").clicked() {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = &state.runtime.last_input_dir {
                dialog = dialog.set_directory(dir);
            }
            if let Some(folder) = dialog.pick_folder() {
                // Remember this folder
                state.runtime.last_input_dir = Some(folder.clone());
                // Recursively add image files from folder
                if let Ok(entries) = std::fs::read_dir(&folder) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && is_supported_image(&path) {
                            state.config.input_paths.push(path);
                        }
                    }
                }
            }
        }
    });

    if !state.config.input_paths.is_empty() {
        ui.horizontal(|ui| {
            if ui.button("Clear All").clicked() {
                state.config.input_paths.clear();
            }
            ui.label(format!("{} file(s)", state.config.input_paths.len()));
        });
    }

    ui.add_space(4.0);

    // File list with remove buttons
    let available_height = ui.available_height() - 120.0; // Reserve space for output controls
    egui::ScrollArea::vertical()
        .max_height(available_height.max(100.0))
        .show(ui, |ui| {
            let mut to_remove = None;
            for (i, path) in state.config.input_paths.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("x").clicked() {
                        to_remove = Some(i);
                    }

                    // Display filename only
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());

                    ui.label(filename);
                });
            }
            if let Some(i) = to_remove {
                state.config.input_paths.remove(i);
            }

            // Empty state
            if state.config.input_paths.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label("Drop images here or use the buttons above");
                });
            }
        });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    // Output section
    ui.horizontal(|ui| {
        ui.label("Output:");
        let path_text = state.config.output_dir.display().to_string();
        ui.add(
            egui::TextEdit::singleline(&mut state.config.output_dir.display().to_string())
                .hint_text("Output directory")
                .desired_width(120.0)
                .interactive(false),
        );

        // Show a shortened path if too long
        if path_text.len() > 20 {
            ui.label("...");
        }

        if ui.button("...").clicked()
            && let Some(folder) = rfd::FileDialog::new()
                .set_directory(&state.config.output_dir)
                .pick_folder()
        {
            state.config.output_dir = folder;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.add(
            egui::TextEdit::singleline(&mut state.config.name)
                .hint_text("atlas")
                .desired_width(150.0),
        );
    });

    ui.add_space(4.0);

    // Format radio buttons
    ui.horizontal(|ui| {
        ui.label("Format:");
        ui.radio_value(&mut state.config.format, OutputFormat::Json, "JSON");
        ui.radio_value(&mut state.config.format, OutputFormat::Godot, "Godot");
    });
}

