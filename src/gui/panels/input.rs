use eframe::egui;

use crate::gui::state::{AppState, OutputFormat, ThumbnailState};
use crate::gui::thumbnail::THUMBNAIL_SIZE;

/// Actions requested by the input panel
#[derive(Default)]
pub struct InputPanelAction {
    pub new_project: bool,
    pub save_config: bool,
    // Dialog requests (run in background threads)
    pub request_open_config_dialog: bool,
    pub request_save_as_dialog: bool,
    pub request_add_files_dialog: bool,
    pub request_add_folder_dialog: bool,
    pub request_output_folder_dialog: bool,
}

/// Input panel with file list, output path, and format selection
pub fn input_panel(ui: &mut egui::Ui, state: &mut AppState) -> InputPanelAction {
    let mut action = InputPanelAction::default();

    // Config file buttons
    ui.horizontal(|ui| {
        if ui.button("New").clicked() {
            action.new_project = true;
        }

        if ui.button("Open").clicked() {
            action.request_open_config_dialog = true;
        }

        // Save button - enabled only if we have a config path
        let can_save = state.runtime.config_path.is_some();
        if ui
            .add_enabled(can_save, egui::Button::new("Save"))
            .clicked()
        {
            action.save_config = true;
        }

        if ui.button("Save As").clicked() {
            action.request_save_as_dialog = true;
        }
    });

    // Show current config path if loaded
    if let Some(path) = &state.runtime.config_path {
        let dirty = if state.runtime.is_config_dirty(&state.config) {
            " *"
        } else {
            ""
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        ui.label(format!("{}{}", name, dirty));
    }

    ui.separator();

    ui.heading("Input Sprites");

    ui.add_space(4.0);

    // File action buttons
    ui.horizontal(|ui| {
        if ui.button("+ Add Files").clicked() {
            action.request_add_files_dialog = true;
        }

        if ui.button("+ Add Folder").clicked() {
            action.request_add_folder_dialog = true;
        }
    });

    if !state.config.input_paths.is_empty() {
        // Clamp selection to valid indices
        let max_idx = state.config.input_paths.len();
        state.runtime.selected_sprites.retain(|&i| i < max_idx);
        if let Some(anchor) = state.runtime.selection_anchor
            && anchor >= max_idx
        {
            state.runtime.selection_anchor = None;
        }

        ui.horizontal(|ui| {
            if ui.button("Clear All").clicked() {
                state.config.input_paths.clear();
                state.runtime.selected_sprites.clear();
                state.runtime.selection_anchor = None;
            }

            let has_selection = !state.runtime.selected_sprites.is_empty();
            if ui
                .add_enabled(has_selection, egui::Button::new("Remove Selected"))
                .clicked()
            {
                remove_selected_sprites(state);
            }

            if has_selection {
                ui.label(format!(
                    "{} selected / {} file(s)",
                    state.runtime.selected_sprites.len(),
                    state.config.input_paths.len()
                ));
            } else {
                ui.label(format!("{} file(s)", state.config.input_paths.len()));
            }
        });

        // Filter input
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.runtime.sprite_filter)
                    .hint_text("Filter sprites...")
                    .desired_width(ui.available_width() - 8.0),
            );
        });
    }

    ui.add_space(4.0);

    // File list
    let available_height = ui.available_height() - 120.0; // Reserve space for output controls
    egui::ScrollArea::vertical()
        .max_height(available_height.max(100.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Filter paths, keeping original indices for removal
            let filter_lower = state.runtime.sprite_filter.to_lowercase();
            let filtered: Vec<(usize, &std::path::PathBuf)> = state
                .config
                .input_paths
                .iter()
                .enumerate()
                .filter(|(_, path)| {
                    if filter_lower.is_empty() {
                        return true;
                    }
                    let filename = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    filename.contains(&filter_lower)
                })
                .collect();

            // Show filtered count if filtering
            if !filter_lower.is_empty() {
                ui.label(format!(
                    "Showing {} of {}",
                    filtered.len(),
                    state.config.input_paths.len()
                ));
            }

            // Get modifiers for selection handling
            let modifiers = ui.input(|i| i.modifiers);

            // Handle Delete/Backspace key (set flag, handle after loop)
            let delete_pressed = ui
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));

            let mut remove_selected = false;

            if delete_pressed && !state.runtime.selected_sprites.is_empty() {
                remove_selected = true;
            }

            let thumb_size = THUMBNAIL_SIZE as f32;

            for (original_idx, path) in &filtered {
                let is_selected = state.runtime.selected_sprites.contains(original_idx);

                // Use Frame to draw selection background before content
                let frame = if is_selected {
                    egui::Frame::none()
                        .fill(ui.visuals().selection.bg_fill)
                        .rounding(2.0)
                } else {
                    egui::Frame::none()
                };

                let row_response = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Thumbnail
                        let (thumb_rect, _) = ui.allocate_exact_size(
                            egui::vec2(thumb_size, thumb_size),
                            egui::Sense::hover(),
                        );

                        match state.runtime.thumbnails.get(*path) {
                            Some(ThumbnailState::Loaded(texture)) => {
                                // Center the texture within the allocated rect
                                let tex_size = texture.size_vec2();
                                let centered_rect = center_rect_in(tex_size, thumb_rect);
                                ui.painter().image(
                                    texture.id(),
                                    centered_rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }
                            Some(ThumbnailState::Loading) => {
                                // Show loading placeholder
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    2.0,
                                    egui::Color32::from_gray(60),
                                );
                            }
                            Some(ThumbnailState::Failed) | None => {
                                // Show error/missing placeholder
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    2.0,
                                    egui::Color32::from_gray(40),
                                );
                                ui.painter().text(
                                    thumb_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "?",
                                    egui::FontId::default(),
                                    egui::Color32::from_gray(80),
                                );
                            }
                        }

                        // Display filename (no click sense, handled by row)
                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());

                        ui.label(filename);
                    })
                });

                // Make entire row clickable by interacting with the frame's rect
                let row_rect = row_response.response.rect;
                let row_id = ui.id().with(original_idx);
                let row_interact = ui.interact(row_rect, row_id, egui::Sense::click());

                if row_interact.clicked() {
                    handle_sprite_click(
                        &mut state.runtime.selected_sprites,
                        &mut state.runtime.selection_anchor,
                        *original_idx,
                        modifiers,
                    );
                }
            }

            // Drop the filtered borrow before modifying state
            drop(filtered);

            // Handle removal of selected items
            if remove_selected {
                remove_selected_sprites(state);
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

        if ui.button("...").clicked() {
            action.request_output_folder_dialog = true;
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
        ui.radio_value(&mut state.config.format, OutputFormat::Tpsheet, "tpsheet");
    });

    action
}

/// Handle click on a sprite row, updating selection based on modifiers
fn handle_sprite_click(
    selected: &mut std::collections::HashSet<usize>,
    anchor: &mut Option<usize>,
    clicked_index: usize,
    modifiers: egui::Modifiers,
) {
    if let Some(anchor_idx) = anchor.filter(|_| modifiers.shift) {
        // Shift+click: select range from anchor to clicked
        let (start, end) = if anchor_idx <= clicked_index {
            (anchor_idx, clicked_index)
        } else {
            (clicked_index, anchor_idx)
        };

        // Add range to selection
        for i in start..=end {
            selected.insert(i);
        }
        // Keep anchor unchanged for shift-select
    } else if modifiers.command {
        // Ctrl/Cmd+click: toggle individual selection
        if selected.contains(&clicked_index) {
            selected.remove(&clicked_index);
        } else {
            selected.insert(clicked_index);
        }
        *anchor = Some(clicked_index);
    } else {
        // Plain click: select only this item
        selected.clear();
        selected.insert(clicked_index);
        *anchor = Some(clicked_index);
    }
}

/// Remove all selected sprites from the input list
fn remove_selected_sprites(state: &mut AppState) {
    let mut indices: Vec<usize> = state.runtime.selected_sprites.iter().copied().collect();
    indices.sort_by(|a, b| b.cmp(a)); // Sort descending

    for i in &indices {
        if *i < state.config.input_paths.len() {
            state.config.input_paths.remove(*i);
        }
    }

    state.runtime.selected_sprites.clear();
    state.runtime.selection_anchor = None;
}

/// Center a smaller rect within a larger rect
fn center_rect_in(inner_size: egui::Vec2, outer: egui::Rect) -> egui::Rect {
    let offset = (outer.size() - inner_size) / 2.0;
    egui::Rect::from_min_size(outer.min + offset, inner_size)
}
