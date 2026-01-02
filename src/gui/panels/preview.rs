use eframe::egui;

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
                    // Reset view when switching atlases
                    state.runtime.preview_zoom = 1.0;
                    state.runtime.preview_offset = egui::Vec2::ZERO;
                }
            }
        });

        ui.separator();
    }

    // Clamp selected atlas to valid range
    let selected = state.runtime.selected_atlas.min(atlases.len() - 1);
    let atlas = &atlases[selected];

    // Stats line with occupancy
    ui.horizontal(|ui| {
        ui.label(format!(
            "{}x{} | {} sprites | {:.1}% occupancy",
            atlas.width,
            atlas.height,
            atlas.sprites.len(),
            atlas.occupancy * 100.0
        ));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Reset view button
            if ui.small_button("Reset View").clicked() {
                state.runtime.preview_zoom = 1.0;
                state.runtime.preview_offset = egui::Vec2::ZERO;
            }

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

    // Draw checkerboard background
    draw_checkerboard(&painter, rect);

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
