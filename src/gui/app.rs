use eframe::egui;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{is_supported_image, panels};
use super::state::{
    AppConfig, AppState, BackgroundTask, Operation, OutputFormat, ResizeMode, Status, StatusResult,
};
use crate::atlas::{Atlas, AtlasBuilder};
use crate::output::{save_atlas_image, write_godot_resources, write_json};
use crate::sprite::load_sprites;

/// Debounce delay for auto-repack (milliseconds)
const AUTO_REPACK_DEBOUNCE_MS: u64 = 300;

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

    /// Poll background pack task for completion
    fn poll_pack_task(&mut self, ctx: &egui::Context) {
        if let Some(task) = &self.state.runtime.pack_task
            && let Some(result) = task.poll()
        {
            // Task completed, clear it
            self.state.runtime.pack_task = None;

            match result {
                Ok(atlases) => {
                    let count = atlases.len();

                    // Create textures from atlases
                    self.state.runtime.atlas_textures = atlases
                        .iter()
                        .enumerate()
                        .map(|(i, atlas)| {
                            let image = egui::ColorImage::from_rgba_unmultiplied(
                                [atlas.width as usize, atlas.height as usize],
                                &atlas.image,
                            );
                            ctx.load_texture(
                                format!("atlas_{}", i),
                                image,
                                egui::TextureOptions::NEAREST,
                            )
                        })
                        .collect();

                    // Reset preview state
                    self.state.runtime.preview_zoom = 1.0;
                    self.state.runtime.preview_offset = egui::Vec2::ZERO;

                    // Store hash for auto-repack detection
                    self.state.runtime.last_packed_hash =
                        Some(self.state.config.pack_settings_hash());

                    self.state.runtime.atlases = Some(atlases);
                    self.state.runtime.selected_atlas = 0;
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Success(format!(
                            "{} atlas{} packed",
                            count,
                            if count == 1 { "" } else { "es" }
                        )),
                        at: Instant::now(),
                    };
                }
                Err(err) => {
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Error(err),
                        at: Instant::now(),
                    };
                }
            }
        }
    }

    /// Start packing in a background thread
    pub fn start_pack(&mut self) {
        // Clone config for the worker thread
        let config = self.state.config.clone();

        // Set up channel
        let (tx, rx) = mpsc::channel();

        // Spawn worker thread
        std::thread::spawn(move || {
            let result = pack_atlases(&config);
            let _ = tx.send(result);
        });

        // Update state
        self.state.runtime.pack_task = Some(BackgroundTask::new(rx));
        self.state.runtime.status = Status::Working {
            operation: Operation::Packing,
            started_at: Instant::now(),
        };
        self.state.runtime.atlases = None; // Clear old atlases
    }

    /// Poll background export task for completion
    fn poll_export_task(&mut self) {
        if let Some(task) = &self.state.runtime.export_task
            && let Some(result) = task.poll()
        {
            // Task completed, clear it
            self.state.runtime.export_task = None;

            match result {
                Ok(()) => {
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Success("Exported successfully".to_string()),
                        at: Instant::now(),
                    };
                }
                Err(err) => {
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Error(err),
                        at: Instant::now(),
                    };
                }
            }
        }
    }

    /// Start export in a background thread
    pub fn start_export(&mut self) {
        // Need atlases to export
        let Some(atlases) = self.state.runtime.atlases.clone() else {
            self.state.runtime.status = Status::Done {
                result: StatusResult::Error("No atlas to export".to_string()),
                at: Instant::now(),
            };
            return;
        };

        // Clone config for the worker thread
        let config = self.state.config.clone();

        // Set up channel
        let (tx, rx) = mpsc::channel();

        // Spawn worker thread
        std::thread::spawn(move || {
            let result = export_atlases(&atlases, &config);
            let _ = tx.send(result);
        });

        // Update state
        self.state.runtime.export_task = Some(BackgroundTask::new(rx));
        self.state.runtime.status = Status::Working {
            operation: Operation::Exporting,
            started_at: Instant::now(),
        };
    }

    /// Handle debounced auto-repack when settings change
    fn handle_auto_repack(&mut self) {
        // Skip if auto-repack is disabled or we're already busy
        if !self.state.runtime.auto_repack {
            self.state.runtime.pending_repack_at = None;
            return;
        }

        if self.state.runtime.pack_task.is_some() || self.state.runtime.export_task.is_some() {
            return;
        }

        // Need files to pack
        if self.state.config.input_paths.is_empty() {
            self.state.runtime.pending_repack_at = None;
            return;
        }

        let current_hash = self.state.config.pack_settings_hash();

        // Check if settings changed since last pack
        let settings_changed = self
            .state
            .runtime
            .last_packed_hash
            .is_none_or(|h| h != current_hash);

        if settings_changed {
            // Schedule or check pending repack
            match self.state.runtime.pending_repack_at {
                Some(pending_at) if Instant::now() >= pending_at => {
                    // Debounce period elapsed, trigger repack
                    self.state.runtime.pending_repack_at = None;
                    self.start_pack();
                }
                Some(_) => {
                    // Still waiting for debounce
                }
                None => {
                    // Schedule a repack after debounce delay
                    self.state.runtime.pending_repack_at =
                        Some(Instant::now() + Duration::from_millis(AUTO_REPACK_DEBOUNCE_MS));
                }
            }
        } else {
            // Settings match last pack, clear any pending repack
            self.state.runtime.pending_repack_at = None;
        }
    }
}

/// Perform packing on a background thread
fn pack_atlases(config: &AppConfig) -> Result<Arc<Vec<Atlas>>, String> {
    if config.input_paths.is_empty() {
        return Err("No input files".to_string());
    }

    // Extract resize options
    let (resize_width, resize_scale) = match config.resize_mode {
        ResizeMode::None => (None, None),
        ResizeMode::Width(w) => (Some(w), None),
        ResizeMode::Scale(s) => (None, Some(s)),
    };

    // Load sprites
    let sprites = load_sprites(
        &config.input_paths,
        config.trim,
        config.trim_margin,
        resize_width,
        resize_scale,
    )
    .map_err(|e| e.to_string())?;

    // Build atlas
    let atlases = AtlasBuilder::new(config.max_width, config.max_height)
        .padding(config.padding)
        .heuristic(config.heuristic)
        .power_of_two(config.pot)
        .extrude(config.extrude)
        .pack_mode(config.pack_mode)
        .build(sprites)
        .map_err(|e| e.to_string())?;

    Ok(Arc::new(atlases))
}

/// Perform export on a background thread
fn export_atlases(atlases: &[Atlas], config: &AppConfig) -> Result<(), String> {
    // Ensure output directory exists
    std::fs::create_dir_all(&config.output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    // Save PNG images for each atlas
    for atlas in atlases {
        let png_path = config
            .output_dir
            .join(format!("{}_{}.png", config.name, atlas.index));
        save_atlas_image(atlas, &png_path, config.opaque, config.compress)
            .map_err(|e| e.to_string())?;
    }

    // Write metadata file based on format
    match config.format {
        OutputFormat::Json => {
            write_json(atlases, &config.output_dir, &config.name).map_err(|e| e.to_string())?;
        }
        OutputFormat::Godot => {
            write_godot_resources(atlases, &config.output_dir, &config.name, None)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

impl eframe::App for BentoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle dropped files
        self.handle_dropped_files(ctx);

        // Poll background tasks
        self.poll_pack_task(ctx);
        self.poll_export_task();

        // Handle auto-repack (debounced)
        self.handle_auto_repack();

        // Request repaint if we have an active task or pending repack
        if self.state.runtime.pack_task.is_some()
            || self.state.runtime.export_task.is_some()
            || self.state.runtime.pending_repack_at.is_some()
        {
            ctx.request_repaint();
        }

        // Auto-clear old success messages
        self.state
            .runtime
            .status
            .maybe_clear(Duration::from_secs(5));

        // Top panel with title/menu bar could go here if needed

        // Bottom panel with Pack/Export buttons and status
        let action = egui::TopBottomPanel::bottom("bottom_bar")
            .show(ctx, |ui| panels::bottom_bar(ui, &mut self.state))
            .inner;

        // Handle actions from bottom bar
        if action.pack_requested {
            self.start_pack();
        }
        if action.export_requested {
            self.start_export();
        }

        // Left panel with input controls
        egui::SidePanel::left("input_panel")
            .default_width(280.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                panels::input_panel(ui, &mut self.state);
            });

        // Right panel with settings
        egui::SidePanel::right("settings_panel")
            .default_width(280.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    panels::settings_panel(ui, &mut self.state);
                });
            });

        // Central panel with preview
        egui::CentralPanel::default().show(ctx, |ui| {
            panels::preview_panel(ui, &mut self.state);
        });

        // Render drag-drop overlay on top of everything
        self.render_drop_overlay(ctx);
    }
}

