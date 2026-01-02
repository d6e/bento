use eframe::egui;

use crate::gui::state::AppState;

/// Preview panel showing the packed atlas (placeholder for Stage 2)
pub fn preview_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Preview");

    ui.add_space(4.0);

    // Check if we have atlases to show
    if let Some(atlases) = &state.runtime.atlases {
        if !atlases.is_empty() {
            // Tab bar for multiple atlases
            if atlases.len() > 1 {
                ui.horizontal(|ui| {
                    for i in 0..atlases.len() {
                        let text = format!("Atlas {}", i);
                        if ui
                            .selectable_label(state.runtime.selected_atlas == i, &text)
                            .clicked()
                        {
                            state.runtime.selected_atlas = i;
                        }
                    }
                });

                ui.separator();
            }

            // Get selected atlas
            let selected = state.runtime.selected_atlas.min(atlases.len() - 1);
            let atlas = &atlases[selected];

            // Stats line
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}x{} | {} sprites",
                    atlas.width,
                    atlas.height,
                    atlas.sprites.len()
                ));
            });

            // Placeholder for actual atlas preview (Stage 3)
            let available = ui.available_size();
            let rect = ui.allocate_space(available).1;
            ui.painter().rect_filled(
                rect,
                4.0,
                egui::Color32::from_gray(40),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Atlas preview will be shown here",
                egui::FontId::default(),
                egui::Color32::from_gray(128),
            );
        } else {
            show_empty_state(ui);
        }
    } else {
        show_empty_state(ui);
    }
}

fn show_empty_state(ui: &mut egui::Ui) {
    let available = ui.available_size();
    let rect = ui.allocate_space(available).1;

    // Draw placeholder background
    ui.painter().rect_filled(
        rect,
        4.0,
        egui::Color32::from_gray(30),
    );

    // Center text
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "No atlas packed yet\n\nAdd images and click 'Pack Atlas'",
        egui::FontId::default(),
        egui::Color32::from_gray(100),
    );
}
