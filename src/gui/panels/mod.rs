mod input;
mod preview;
mod settings;

pub use input::input_panel;
pub use preview::preview_panel;
pub use settings::settings_panel;

use eframe::egui;

use super::state::{AppState, Operation, Status, StatusResult};

/// Action requested by the bottom bar
#[derive(Default)]
pub struct BottomBarAction {
    pub pack_requested: bool,
    pub cancel_requested: bool,
    pub export_requested: bool,
}

/// Bottom bar with Pack/Export buttons and status
pub fn bottom_bar(ui: &mut egui::Ui, state: &mut AppState) -> BottomBarAction {
    let mut action = BottomBarAction::default();

    ui.horizontal(|ui| {
        let is_packing = matches!(
            state.runtime.status,
            Status::Working {
                operation: Operation::Packing,
                ..
            }
        );
        let is_busy = matches!(state.runtime.status, Status::Working { .. });
        let has_files = !state.config.input_paths.is_empty();

        // Pack/Cancel button
        if is_packing {
            if ui
                .add(egui::Button::new("Cancel").fill(egui::Color32::from_rgb(180, 60, 60)))
                .clicked()
            {
                action.cancel_requested = true;
            }
        } else if ui
            .add_enabled(!is_busy && has_files, egui::Button::new("Pack Atlas"))
            .clicked()
        {
            action.pack_requested = true;
        }

        ui.checkbox(&mut state.runtime.auto_repack, "Auto");

        if is_busy {
            ui.spinner();
        }

        ui.separator();

        // Status text
        let status_text = match &state.runtime.status {
            Status::Idle => {
                if has_files {
                    "Ready".to_string()
                } else {
                    "Add images to pack".to_string()
                }
            }
            Status::Working { operation, .. } => match operation {
                Operation::Packing => "Packing...".to_string(),
                Operation::Exporting => "Exporting...".to_string(),
            },
            Status::Done { result, .. } => match result {
                StatusResult::Success(msg) => msg.clone(),
                StatusResult::Error(err) => format!("Error: {}", err),
            },
        };

        // Color status text based on result
        let text_color = match &state.runtime.status {
            Status::Done {
                result: StatusResult::Error(_),
                ..
            } => Some(egui::Color32::from_rgb(255, 100, 100)),
            Status::Done {
                result: StatusResult::Success(_),
                ..
            } => Some(egui::Color32::from_rgb(100, 200, 100)),
            _ => None,
        };

        if let Some(color) = text_color {
            ui.colored_label(color, status_text);
        } else {
            ui.label(status_text);
        }

        // Export button on the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can_export = !is_busy && state.runtime.atlases.is_some();
            if ui
                .add_enabled(can_export, egui::Button::new("Export"))
                .clicked()
            {
                action.export_requested = true;
            }
        });
    });

    action
}
