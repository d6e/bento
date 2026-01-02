use eframe::egui;

use crate::cli::{CompressionLevel, PackMode, PackingHeuristic};
use crate::gui::state::{AppState, ResizeMode};

/// Settings panel with all packing/export options
pub fn settings_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Settings");

    ui.add_space(4.0);

    // Atlas section
    egui::CollapsingHeader::new("Atlas")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Max Width:");
                ui.add(
                    egui::DragValue::new(&mut state.config.max_width)
                        .range(64..=16384)
                        .speed(64),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Max Height:");
                ui.add(
                    egui::DragValue::new(&mut state.config.max_height)
                        .range(64..=16384)
                        .speed(64),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Padding:");
                ui.add(
                    egui::DragValue::new(&mut state.config.padding)
                        .range(0..=32)
                        .speed(1),
                );
            });

            ui.checkbox(&mut state.config.pot, "Power of Two");
        });

    // Sprites section
    egui::CollapsingHeader::new("Sprites")
        .default_open(true)
        .show(ui, |ui| {
            ui.checkbox(&mut state.config.trim, "Trim transparent borders");

            if state.config.trim {
                ui.horizontal(|ui| {
                    ui.label("Trim Margin:");
                    ui.add(
                        egui::DragValue::new(&mut state.config.trim_margin)
                            .range(0..=32)
                            .speed(1),
                    );
                });
            }

            ui.horizontal(|ui| {
                ui.label("Extrude:");
                ui.add(
                    egui::DragValue::new(&mut state.config.extrude)
                        .range(0..=8)
                        .speed(1),
                );
            });

            // Resize mode
            ui.horizontal(|ui| {
                ui.label("Resize:");
                let current = match state.config.resize_mode {
                    ResizeMode::None => 0,
                    ResizeMode::Width(_) => 1,
                    ResizeMode::Scale(_) => 2,
                };

                let mut selected = current;
                egui::ComboBox::from_id_salt("resize_mode")
                    .selected_text(match current {
                        0 => "None",
                        1 => "Width",
                        _ => "Scale",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "None");
                        ui.selectable_value(&mut selected, 1, "Width");
                        ui.selectable_value(&mut selected, 2, "Scale");
                    });

                // Update resize mode if selection changed
                if selected != current {
                    state.config.resize_mode = match selected {
                        0 => ResizeMode::None,
                        1 => ResizeMode::Width(256),
                        _ => ResizeMode::Scale(0.5),
                    };
                }
            });

            // Show value input based on resize mode
            match &mut state.config.resize_mode {
                ResizeMode::None => {}
                ResizeMode::Width(width) => {
                    ui.horizontal(|ui| {
                        ui.label("Target Width:");
                        ui.add(egui::DragValue::new(width).range(1..=4096).speed(1));
                        ui.label("px");
                    });
                }
                ResizeMode::Scale(scale) => {
                    ui.horizontal(|ui| {
                        ui.label("Scale Factor:");
                        ui.add(
                            egui::DragValue::new(scale)
                                .range(0.01..=4.0)
                                .speed(0.01)
                                .fixed_decimals(2),
                        );
                    });
                }
            }
        });

    // Packing section
    egui::CollapsingHeader::new("Packing")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Heuristic:");
                egui::ComboBox::from_id_salt("heuristic")
                    .selected_text(heuristic_name(state.config.heuristic))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::BestShortSideFit,
                            "Best Short Side",
                        );
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::BestLongSideFit,
                            "Best Long Side",
                        );
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::BestAreaFit,
                            "Best Area",
                        );
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::BottomLeft,
                            "Bottom Left",
                        );
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::ContactPoint,
                            "Contact Point",
                        );
                        ui.selectable_value(
                            &mut state.config.heuristic,
                            PackingHeuristic::Best,
                            "Best (try all)",
                        );
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Pack Mode:");
                egui::ComboBox::from_id_salt("pack_mode")
                    .selected_text(pack_mode_name(state.config.pack_mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.config.pack_mode, PackMode::Single, "Single");
                        ui.selectable_value(&mut state.config.pack_mode, PackMode::Best, "Best");
                    });
            });
        });

    // Output section
    egui::CollapsingHeader::new("Output")
        .default_open(true)
        .show(ui, |ui| {
            ui.checkbox(&mut state.config.opaque, "Opaque (RGB instead of RGBA)");

            // Compression
            let compress_enabled = state.config.compress.is_some();
            let mut compress_checkbox = compress_enabled;

            ui.horizontal(|ui| {
                if ui.checkbox(&mut compress_checkbox, "Compress PNG").changed() {
                    state.config.compress = if compress_checkbox {
                        Some(CompressionLevel::Level(2))
                    } else {
                        None
                    };
                }
            });

            if let Some(ref mut level) = state.config.compress {
                ui.horizontal(|ui| {
                    ui.label("Level:");
                    let current = match level {
                        CompressionLevel::Level(n) => *n as i32,
                        CompressionLevel::Max => 7,
                    };

                    let mut selected = current;
                    egui::ComboBox::from_id_salt("compress_level")
                        .selected_text(match current {
                            7 => "Max".to_string(),
                            n => n.to_string(),
                        })
                        .show_ui(ui, |ui| {
                            for i in 0..=6 {
                                ui.selectable_value(&mut selected, i, i.to_string());
                            }
                            ui.selectable_value(&mut selected, 7, "Max");
                        });

                    if selected != current {
                        *level = if selected == 7 {
                            CompressionLevel::Max
                        } else {
                            CompressionLevel::Level(selected as u8)
                        };
                    }
                });
            }
        });
}

fn heuristic_name(h: PackingHeuristic) -> &'static str {
    match h {
        PackingHeuristic::BestShortSideFit => "Best Short Side",
        PackingHeuristic::BestLongSideFit => "Best Long Side",
        PackingHeuristic::BestAreaFit => "Best Area",
        PackingHeuristic::BottomLeft => "Bottom Left",
        PackingHeuristic::ContactPoint => "Contact Point",
        PackingHeuristic::Best => "Best (try all)",
    }
}

fn pack_mode_name(m: PackMode) -> &'static str {
    match m {
        PackMode::Single => "Single",
        PackMode::Best => "Best",
    }
}
