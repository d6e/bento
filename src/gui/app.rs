use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::state::{
    AppConfig, AppState, BackgroundTask, Operation, OutputFormat, PackResult, ResizeMode, Status,
    StatusResult, ThumbnailState,
};
use super::thumbnail::spawn_thumbnail_loader;
use super::{is_supported_image, panels};
use crate::atlas::{Atlas, AtlasBuilder};
use crate::cli::CompressionLevel;
use crate::output::{save_atlas_image, write_godot_resources, write_json};
use crate::sprite::load_sprites;

/// Debounce delay for auto-repack (milliseconds)
const AUTO_REPACK_DEBOUNCE_MS: u64 = 300;

/// Main GUI application
pub struct BentoApp {
    state: AppState,
}

const LAST_INPUT_DIR_KEY: &str = "last_input_dir";

impl BentoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut state = AppState::default();

        // Restore persisted state
        if let Some(storage) = cc.storage {
            state.runtime.last_input_dir = eframe::get_value(storage, LAST_INPUT_DIR_KEY);
        }

        Self { state }
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
                Ok(pack_result) => {
                    let count = pack_result.atlases.len();

                    // Create textures from atlases
                    self.state.runtime.atlas_textures = pack_result
                        .atlases
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

                    // Use pre-computed PNG sizes from background thread
                    self.state.runtime.atlas_png_sizes = pack_result.png_sizes;

                    // Store hashes for auto-repack detection
                    self.state.runtime.last_packed_hash =
                        Some(self.state.config.pack_settings_hash());
                    self.state.runtime.last_export_hash =
                        Some(self.state.config.export_settings_hash());

                    self.state.runtime.atlases = Some(pack_result.atlases);
                    self.state.runtime.selected_atlas = 0;
                    self.state.runtime.needs_fit_to_view = true;
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Success(format!(
                            "{} atlas{} packed",
                            count,
                            if count == 1 { "" } else { "es" }
                        )),
                        at: Instant::now(),
                    };
                }
                Err(err) if err.contains("cancelled") => {
                    // Cancelled - return to idle, discard results
                    self.state.runtime.status = Status::Idle;
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

        // Set up channel and cancel token
        let (tx, rx) = mpsc::channel();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let token_clone = cancel_token.clone();

        // Spawn worker thread
        std::thread::spawn(move || {
            let result = pack_atlases(&config, token_clone);
            let _ = tx.send(result);
        });

        // Update state
        self.state.runtime.pack_task = Some(BackgroundTask::with_cancel_token(rx, cancel_token));
        self.state.runtime.status = Status::Working {
            operation: Operation::Packing,
            started_at: Instant::now(),
        };
        self.state.runtime.atlases = None; // Clear old atlases
    }

    /// Cancel the current packing operation
    pub fn cancel_pack(&mut self) {
        if let Some(task) = &self.state.runtime.pack_task {
            task.cancel();
        }
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
                    let path = self.state.config.output_dir.display();
                    self.state.runtime.status = Status::Done {
                        result: StatusResult::Success(format!("Exported to {}", path)),
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

    /// Re-estimate PNG sizes when export settings change without triggering a full rebuild
    fn handle_export_settings_change(&mut self) {
        let current_export_hash = self.state.config.export_settings_hash();

        // Check if export settings changed since last estimation
        let export_changed = self
            .state
            .runtime
            .last_export_hash
            .is_none_or(|h| h != current_export_hash);

        if !export_changed {
            return;
        }

        // Only re-estimate if we have atlases
        let Some(atlases) = &self.state.runtime.atlases else {
            return;
        };

        // Re-estimate PNG sizes with new export settings
        let opaque = self.state.config.opaque;
        let compress = self.state.config.compress;
        self.state.runtime.atlas_png_sizes = atlases
            .iter()
            .map(|a| estimate_png_size(&a.image, opaque, compress))
            .collect();

        self.state.runtime.last_export_hash = Some(current_export_hash);
    }

    /// Queue thumbnail loading for paths that aren't in the cache
    fn queue_thumbnail_loading(&mut self) {
        // Collect paths that need loading
        let paths_to_load: Vec<std::path::PathBuf> = self
            .state
            .config
            .input_paths
            .iter()
            .filter(|p| !self.state.runtime.thumbnails.contains_key(*p))
            .cloned()
            .collect();

        if paths_to_load.is_empty() {
            return;
        }

        // Mark as loading
        for path in &paths_to_load {
            self.state
                .runtime
                .thumbnails
                .insert(path.clone(), ThumbnailState::Loading);
        }

        // Spawn loader if not already running
        if self.state.runtime.thumbnail_receiver.is_none() {
            self.state.runtime.thumbnail_receiver = Some(spawn_thumbnail_loader(paths_to_load));
        }
    }

    /// Poll for completed thumbnail loads
    fn poll_thumbnails(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.state.runtime.thumbnail_receiver else {
            return;
        };

        // Drain all available results
        loop {
            match receiver.try_recv() {
                Ok((path, image)) => {
                    let state = match image {
                        Some(img) => {
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                img.as_raw(),
                            );
                            let texture = ctx.load_texture(
                                format!("thumb_{}", path.display()),
                                color_image,
                                egui::TextureOptions::LINEAR,
                            );
                            ThumbnailState::Loaded(texture)
                        }
                        None => ThumbnailState::Failed,
                    };
                    self.state.runtime.thumbnails.insert(path, state);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Loader thread finished
                    self.state.runtime.thumbnail_receiver = None;

                    // Check if there are new paths that need loading
                    self.queue_thumbnail_loading();
                    break;
                }
            }
        }
    }

    /// Clean up thumbnails for paths no longer in input_paths
    fn cleanup_thumbnails(&mut self) {
        self.state.runtime.thumbnails.retain(|path, _| {
            self.state.config.input_paths.contains(path)
        });
    }
}

/// Perform packing on a background thread
fn pack_atlases(config: &AppConfig, cancel_token: Arc<AtomicBool>) -> Result<PackResult, String> {
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
        .cancel_token(cancel_token)
        .build(sprites)
        .map_err(|e| e.to_string())?;

    // Estimate PNG sizes on background thread to avoid blocking UI
    let png_sizes: Vec<usize> = atlases
        .iter()
        .map(|a| estimate_png_size(&a.image, config.opaque, config.compress))
        .collect();

    Ok(PackResult {
        atlases: Arc::new(atlases),
        png_sizes,
    })
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

/// Estimate PNG file size by encoding to memory, optionally with compression
fn estimate_png_size(
    image: &image::RgbaImage,
    opaque: bool,
    compress: Option<CompressionLevel>,
) -> usize {
    use image::codecs::png::PngEncoder;
    use image::{DynamicImage, ImageEncoder};
    use std::io::Cursor;

    let mut buffer = Cursor::new(Vec::new());

    // Handle opaque conversion (RGB vs RGBA)
    let encode_result = if opaque {
        let rgb = DynamicImage::ImageRgba8(image.clone()).into_rgb8();
        let encoder = PngEncoder::new(&mut buffer);
        encoder.write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
    } else {
        let encoder = PngEncoder::new(&mut buffer);
        encoder.write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
    };

    if encode_result.is_err() {
        return 0;
    }

    // Apply compression if enabled
    if let Some(level) = compress {
        let opts = match level {
            CompressionLevel::Level(n) => oxipng::Options::from_preset(n),
            CompressionLevel::Max => oxipng::Options::max_compression(),
        };
        match oxipng::optimize_from_memory(&buffer.into_inner(), &opts) {
            Ok(compressed) => compressed.len(),
            Err(_) => 0,
        }
    } else {
        buffer.into_inner().len()
    }
}

impl eframe::App for BentoApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, LAST_INPUT_DIR_KEY, &self.state.runtime.last_input_dir);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle dropped files
        self.handle_dropped_files(ctx);

        // Poll background tasks
        self.poll_pack_task(ctx);
        self.poll_export_task();

        // Handle thumbnails
        self.queue_thumbnail_loading();
        self.poll_thumbnails(ctx);
        self.cleanup_thumbnails();

        // Handle auto-repack (debounced)
        self.handle_auto_repack();

        // Re-estimate PNG sizes if export settings changed
        self.handle_export_settings_change();

        // Request repaint if we have an active task or pending repack
        if self.state.runtime.pack_task.is_some()
            || self.state.runtime.export_task.is_some()
            || self.state.runtime.pending_repack_at.is_some()
            || self.state.runtime.thumbnail_receiver.is_some()
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
        if action.cancel_requested {
            self.cancel_pack();
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

