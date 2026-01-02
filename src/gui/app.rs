use eframe::egui;
use std::time::Duration;

use super::{is_supported_image, panels};
use super::state::AppState;

/// Main GUI application
pub struct BentoApp {
    state: AppState,
}

impl BentoApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::default(),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    // Add files directly, or recursively add from directories
                    if path.is_dir() {
                        if let Ok(entries) = std::fs::read_dir(path) {
                            for entry in entries.flatten() {
                                let entry_path = entry.path();
                                if entry_path.is_file() && is_supported_image(&entry_path) {
                                    self.state.config.input_paths.push(entry_path);
                                }
                            }
                        }
                    } else if is_supported_image(path) {
                        self.state.config.input_paths.push(path.clone());
                    }
                }
            }
        });
    }

    fn render_drop_overlay(&self, ctx: &egui::Context) {
        let is_hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        if is_hovering {
            let screen_rect = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            ));
            painter.rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(100, 150, 255, 40),
            );
            painter.rect_stroke(
                screen_rect,
                0.0,
                egui::Stroke::new(3.0, egui::Color32::from_rgb(100, 150, 255)),
            );
        }
    }
}

impl eframe::App for BentoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle dropped files
        self.handle_dropped_files(ctx);

        // Auto-clear old success messages
        self.state
            .runtime
            .status
            .maybe_clear(Duration::from_secs(5));

        // Top panel with title/menu bar could go here if needed

        // Bottom panel with Pack/Export buttons and status
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            panels::bottom_bar(ui, &mut self.state);
        });

        // Left panel with input controls
        egui::SidePanel::left("input_panel")
            .default_width(280.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                panels::input_panel(ui, &mut self.state);
            });

        // Right side split: preview on top, settings on bottom
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_height = ui.available_height();
            let preview_height = available_height * 0.6;

            // Preview panel (top)
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), preview_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    panels::preview_panel(ui, &mut self.state);
                },
            );

            ui.separator();

            // Settings panel (bottom)
            egui::ScrollArea::vertical().show(ui, |ui| {
                panels::settings_panel(ui, &mut self.state);
            });
        });

        // Render drag-drop overlay on top of everything
        self.render_drop_overlay(ctx);
    }
}

