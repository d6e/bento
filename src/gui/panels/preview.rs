use eframe::egui;

use crate::atlas::Atlas;
use crate::gui::state::AppState;

/// Preview panel showing the packed atlas with zoom/pan support
pub fn preview_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Preview");

    ui.add_space(4.0);

    // Check if we're currently packing
    let is_packing = state.runtime.pack_task.is_some();

    // Check if we have atlases to show
    let Some(atlases) = state.runtime.atlases.as_ref().filter(|a| !a.is_empty()) else {
        if is_packing {
            show_packing_state(ui);
        } else {
            show_empty_state(ui);
        }
        return;
    };

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
                    // Fit view when switching atlases
                    state.runtime.needs_fit_to_view = true;
                }
            }
        });

        ui.separator();
    }

    // Clamp selected atlas to valid range
    let selected = state.runtime.selected_atlas.min(atlases.len() - 1);
    let atlas = &atlases[selected];

    // Stats line with occupancy and file size
    let file_size = state
        .runtime
        .atlas_png_sizes
        .get(selected)
        .copied()
        .unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(format!(
            "{}x{} | {} sprites | {:.1}% occupancy | {}",
            atlas.width,
            atlas.height,
            atlas.sprites.len(),
            atlas.occupancy * 100.0,
            format_file_size(file_size)
        ));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Reset view button (fits atlas to view)
            if ui.small_button("Reset View").clicked() {
                state.runtime.needs_fit_to_view = true;
            }

            // Debug overlay toggle
            ui.checkbox(&mut state.runtime.show_debug_overlay, "Debug");

            // Zoom display
            ui.label(format!("{:.0}%", state.runtime.preview_zoom * 100.0));
        });
    });

    ui.add_space(4.0);

    // Get texture for selected atlas
    if selected >= state.runtime.atlas_textures.len() {
        show_empty_state(ui);
        return;
    }

    let texture = &state.runtime.atlas_textures[selected];

    // Preview area with zoom/pan
    let available = ui.available_size();
    let (response, mut painter) =
        ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;

    // Apply fit-to-view if requested
    if state.runtime.needs_fit_to_view {
        state.runtime.preview_zoom = calculate_fit_zoom(atlas.width, atlas.height, available, 40.0);
        state.runtime.preview_offset = egui::Vec2::ZERO;
        state.runtime.needs_fit_to_view = false;
    }

    // Draw background - solid black when opaque, checkerboard otherwise
    if state.config.opaque {
        painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
    } else {
        draw_checkerboard(&painter, rect);
    }

    // Handle zoom with scroll
    let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
    if scroll_delta != 0.0 && response.hovered() {
        let zoom_factor = 1.1_f32.powf(scroll_delta / 50.0);
        let new_zoom = (state.runtime.preview_zoom * zoom_factor).clamp(0.1, 10.0);

        // Zoom toward mouse position
        if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
            let rel_pos = pointer_pos - rect.center() - state.runtime.preview_offset;
            let scale_change = new_zoom / state.runtime.preview_zoom;
            state.runtime.preview_offset -= rel_pos * (scale_change - 1.0);
        }

        state.runtime.preview_zoom = new_zoom;
    }

    // Handle pan with drag
    if response.dragged() {
        state.runtime.preview_offset += response.drag_delta();
    }

    // Calculate image rect with zoom and offset
    let zoom = state.runtime.preview_zoom;
    let img_size = egui::vec2(atlas.width as f32 * zoom, atlas.height as f32 * zoom);
    let img_center = rect.center() + state.runtime.preview_offset;
    let img_rect = egui::Rect::from_center_size(img_center, img_size);

    // Clip to preview area
    painter.set_clip_rect(rect);

    // Draw the atlas texture
    painter.image(
        texture.id(),
        img_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    // Draw border around atlas
    painter.rect_stroke(
        img_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
    );

    // Draw debug overlay if enabled
    if state.runtime.show_debug_overlay {
        draw_debug_overlay(
            &painter,
            atlas,
            img_rect,
            zoom,
            state.config.padding,
            state.config.extrude,
        );
    }

    // Sprite hover tooltip
    if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
        if img_rect.contains(pointer_pos) {
            // Convert screen position to atlas coordinates
            let atlas_x = (pointer_pos.x - img_rect.left()) / zoom;
            let atlas_y = (pointer_pos.y - img_rect.top()) / zoom;

            // Find sprite under cursor
            for sprite in &atlas.sprites {
                let sprite_rect = egui::Rect::from_min_size(
                    egui::pos2(sprite.x as f32, sprite.y as f32),
                    egui::vec2(sprite.width as f32, sprite.height as f32),
                );

                if sprite_rect.contains(egui::pos2(atlas_x, atlas_y)) {
                    // Build tooltip text
                    let trim_info = &sprite.trim_info;
                    let tooltip_text = if trim_info.was_trimmed() {
                        format!(
                            "{}\n{}x{} (trimmed from {}x{})\nOffset: ({}, {})",
                            sprite.name,
                            sprite.width,
                            sprite.height,
                            trim_info.source_width,
                            trim_info.source_height,
                            trim_info.offset_x,
                            trim_info.offset_y
                        )
                    } else {
                        format!("{}\n{}x{}", sprite.name, sprite.width, sprite.height)
                    };

                    response.clone().on_hover_ui_at_pointer(|ui| {
                        ui.label(tooltip_text);
                    });
                    break;
                }
            }
        }
    }
}

fn show_empty_state(ui: &mut egui::Ui) {
    let available = ui.available_size();
    let rect = ui.allocate_space(available).1;

    // Draw placeholder background
    ui.painter()
        .rect_filled(rect, 4.0, egui::Color32::from_gray(30));

    // Center text
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "No atlas packed yet\n\nAdd images and click 'Pack Atlas'",
        egui::FontId::default(),
        egui::Color32::from_gray(100),
    );
}

fn show_packing_state(ui: &mut egui::Ui) {
    let available = ui.available_size();
    let rect = ui.allocate_space(available).1;

    // Draw placeholder background
    ui.painter()
        .rect_filled(rect, 4.0, egui::Color32::from_gray(30));

    // Draw spinner
    let center = rect.center();
    let time = ui.input(|i| i.time);
    let radius = 16.0;
    let stroke_width = 3.0;

    // Spinning arc
    let start_angle = (time * 2.0) as f32;
    let arc_length = std::f32::consts::PI * 1.5;

    let points: Vec<egui::Pos2> = (0..32)
        .map(|i| {
            let angle = start_angle + (i as f32 / 31.0) * arc_length;
            egui::pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();

    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(stroke_width, egui::Color32::from_gray(150)),
    ));

    // Text below spinner
    ui.painter().text(
        egui::pos2(center.x, center.y + 32.0),
        egui::Align2::CENTER_CENTER,
        "Packing...",
        egui::FontId::default(),
        egui::Color32::from_gray(100),
    );

    // Request continuous repaints for animation
    ui.ctx().request_repaint();
}

/// Draw a checkerboard background to show transparency
fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect) {
    let checker_size: f32 = 8.0;
    let color1 = egui::Color32::from_gray(45);
    let color2 = egui::Color32::from_gray(55);

    // Fill with base color first
    painter.rect_filled(rect, 0.0, color1);

    // Draw checker pattern
    let start_x = rect.left();
    let start_y = rect.top();
    let mut row = 0;

    let mut y = start_y;
    while y < rect.bottom() {
        let mut col = row % 2;
        let mut x = start_x;
        while x < rect.right() {
            if col % 2 == 1 {
                let checker_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(
                        checker_size.min(rect.right() - x),
                        checker_size.min(rect.bottom() - y),
                    ),
                );
                painter.rect_filled(checker_rect, 0.0, color2);
            }
            x += checker_size;
            col += 1;
        }
        y += checker_size;
        row += 1;
    }
}

/// Format file size in human-readable form
fn format_file_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Calculate zoom level that fits the atlas within the canvas with margin
fn calculate_fit_zoom(atlas_width: u32, atlas_height: u32, canvas_size: egui::Vec2, margin: f32) -> f32 {
    let available_width = (canvas_size.x - margin * 2.0).max(1.0);
    let available_height = (canvas_size.y - margin * 2.0).max(1.0);

    let zoom_x = available_width / atlas_width as f32;
    let zoom_y = available_height / atlas_height as f32;

    // Use the smaller zoom to ensure entire atlas fits
    zoom_x.min(zoom_y).clamp(0.1, 10.0)
}

/// Draw debug overlay showing sprite bounds, extrusion, and padding regions
fn draw_debug_overlay(
    painter: &egui::Painter,
    atlas: &Atlas,
    img_rect: egui::Rect,
    zoom: f32,
    padding: u32,
    extrude: u32,
) {
    // Colors for different regions (semi-transparent)
    let sprite_color = egui::Color32::from_rgba_unmultiplied(0, 255, 0, 180); // Green
    let extrude_color = egui::Color32::from_rgba_unmultiplied(255, 165, 0, 120); // Orange
    let padding_color = egui::Color32::from_rgba_unmultiplied(255, 0, 255, 80); // Magenta

    let padding_f = padding as f32;
    let extrude_f = extrude as f32;

    for sprite in &atlas.sprites {
        // Calculate screen coordinates for sprite content
        let sprite_x = img_rect.left() + sprite.x as f32 * zoom;
        let sprite_y = img_rect.top() + sprite.y as f32 * zoom;
        let sprite_w = sprite.width as f32 * zoom;
        let sprite_h = sprite.height as f32 * zoom;

        // 1. Draw padding region (outermost) if padding > 0
        if padding > 0 {
            let padding_offset = (padding_f + extrude_f) * zoom;
            let padding_rect = egui::Rect::from_min_size(
                egui::pos2(sprite_x - padding_offset, sprite_y - padding_offset),
                egui::vec2(
                    sprite_w + 2.0 * padding_offset,
                    sprite_h + 2.0 * padding_offset,
                ),
            );
            painter.rect_stroke(padding_rect, 0.0, egui::Stroke::new(1.0, padding_color));
        }

        // 2. Draw extrusion region if extrude > 0
        if extrude > 0 {
            let extrude_offset = extrude_f * zoom;
            let extrude_rect = egui::Rect::from_min_size(
                egui::pos2(sprite_x - extrude_offset, sprite_y - extrude_offset),
                egui::vec2(
                    sprite_w + 2.0 * extrude_offset,
                    sprite_h + 2.0 * extrude_offset,
                ),
            );
            painter.rect_stroke(extrude_rect, 0.0, egui::Stroke::new(1.0, extrude_color));
        }

        // 3. Draw sprite content boundary (innermost)
        let sprite_rect = egui::Rect::from_min_size(
            egui::pos2(sprite_x, sprite_y),
            egui::vec2(sprite_w, sprite_h),
        );
        painter.rect_stroke(sprite_rect, 0.0, egui::Stroke::new(1.5, sprite_color));
    }
}
