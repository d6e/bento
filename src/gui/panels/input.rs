use eframe::egui;

use crate::gui::is_supported_image;
use crate::gui::state::{AppState, OutputFormat, ThumbnailState};
use crate::gui::thumbnail::THUMBNAIL_SIZE;

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

    // File list with remove buttons
    let available_height = ui.available_height() - 120.0; // Reserve space for output controls
    egui::ScrollArea::vertical()
        .max_height(available_height.max(100.0))
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

            let mut to_remove = None;
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
                        // Remove button (x) for quick single removal
                        if ui.small_button("x").clicked() {
                            to_remove = Some(*original_idx);
                        }

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
            } else if let Some(i) = to_remove {
                // Remove single item and adjust selection
                state.config.input_paths.remove(i);
                adjust_selection_after_removal(
                    &mut state.runtime.selected_sprites,
                    &mut state.runtime.selection_anchor,
                    &[i],
                );
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
        ui.radio_value(&mut state.config.format, OutputFormat::Tpsheet, "tpsheet");
    });
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

/// Adjust selection indices after items are removed
fn adjust_selection_after_removal(
    selected: &mut std::collections::HashSet<usize>,
    anchor: &mut Option<usize>,
    removed_indices: &[usize],
) {
    // Sort indices in descending order for stable adjustment
    let mut sorted_removed: Vec<_> = removed_indices.to_vec();
    sorted_removed.sort_by(|a, b| b.cmp(a));

    let mut new_selected = std::collections::HashSet::new();
    for &idx in selected.iter() {
        let mut adjusted = idx;
        let mut was_removed = false;
        for &removed in &sorted_removed {
            if removed < idx {
                adjusted -= 1;
            } else if removed == idx {
                was_removed = true;
                break;
            }
        }
        if !was_removed {
            new_selected.insert(adjusted);
        }
    }
    *selected = new_selected;

    // Adjust anchor similarly
    if let Some(a) = *anchor {
        let mut adjusted = a;
        for &removed in &sorted_removed {
            if removed < a {
                adjusted -= 1;
            } else if removed == a {
                *anchor = None;
                return;
            }
        }
        *anchor = Some(adjusted);
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
