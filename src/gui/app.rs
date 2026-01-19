use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::dialogs::{
    ConfigChooserDialog, PendingAction, UnsavedChangesChoice, UnsavedChangesDialog,
    find_bento_files,
};
use super::state::{
    AppConfig, AppState, BackgroundTask, Operation, OutputFormat, PackResult, ResizeMode, Status,
    StatusResult, ThumbnailState,
};
use super::thumbnail::spawn_thumbnail_loader;
use super::{is_supported_image, panels};
use crate::atlas::{Atlas, AtlasBuilder};
use crate::cli::{CompressionLevel, PackMode, PackingHeuristic};
use crate::config::{BentoConfig, LoadedConfig, save_config};
use crate::output::{save_atlas_image, write_godot_resources, write_json, write_tpsheet};
use crate::sprite::load_sprites;

/// Debounce delay for auto-repack (milliseconds)
const AUTO_REPACK_DEBOUNCE_MS: u64 = 300;

/// Main GUI application
pub struct BentoApp {
    state: AppState,
    config_chooser: Option<ConfigChooserDialog>,
    unsaved_changes_dialog: Option<UnsavedChangesDialog>,
    /// Set to true when user confirms they want to close (after save/discard dialog)
    allowed_to_close: bool,
}

const LAST_INPUT_DIR_KEY: &str = "last_input_dir";

impl BentoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        let mut app = Self {
            state: AppState::default(),
            config_chooser: None,
            unsaved_changes_dialog: None,
            allowed_to_close: false,
        };

        // Restore persisted state
        if let Some(storage) = cc.storage {
            app.state.runtime.last_input_dir = eframe::get_value(storage, LAST_INPUT_DIR_KEY);
        }

        // Handle initial path
        if let Some(path) = initial_path {
            app.handle_initial_path(path);
        }

        app
    }

    fn handle_initial_path(&mut self, path: PathBuf) {
        if path.is_file() && path.extension().is_some_and(|e| e == "bento") {
            // Direct .bento file - load it
            self.load_config_file(&path);
        } else if path.is_dir() {
            // Directory - look for .bento files
            let bento_files = find_bento_files(&path);
            match bento_files.len() {
                0 => {
                    // No .bento files, maybe add images from directory
                    self.state.runtime.last_input_dir = Some(path);
                }
                1 => {
                    // Single .bento file - load it automatically
                    self.load_config_file(&bento_files[0]);
                }
                _ => {
                    // Multiple .bento files - show chooser dialog
                    self.config_chooser = Some(ConfigChooserDialog::new(bento_files));
                }
            }
        }
    }

    fn load_config_file(&mut self, path: &std::path::Path) {
        match LoadedConfig::load(path) {
            Ok(loaded) => {
                self.apply_loaded_config(loaded, path.to_path_buf());
            }
            Err(e) => {
                self.state.runtime.status = Status::Done {
                    result: StatusResult::Error(format!("Failed to load config: {}", e)),
                    at: std::time::Instant::now(),
                };
            }
        }
    }

    fn apply_loaded_config(&mut self, loaded: LoadedConfig, config_path: PathBuf) {
        let cfg = &loaded.config;

        // Resolve input paths
        match loaded.resolve_inputs() {
            Ok(paths) => self.state.config.input_paths = paths,
            Err(e) => {
                self.state.runtime.status = Status::Done {
                    result: StatusResult::Error(format!("Failed to resolve inputs: {}", e)),
                    at: std::time::Instant::now(),
                };
                return;
            }
        }

        // Apply settings
        self.state.config.output_dir = loaded.resolve_output_dir();
        self.state.config.name = cfg.name.clone();
        self.state.config.format = match cfg.format.as_deref() {
            Some("godot") => OutputFormat::Godot,
            Some("tpsheet") => OutputFormat::Tpsheet,
            _ => OutputFormat::Json,
        };
        self.state.config.max_width = cfg.max_width;
        self.state.config.max_height = cfg.max_height;
        self.state.config.padding = cfg.padding;
        self.state.config.pot = cfg.pot;
        self.state.config.trim = cfg.trim;
        self.state.config.trim_margin = cfg.trim_margin;
        self.state.config.extrude = cfg.extrude;

        // Resize mode
        self.state.config.resize_mode = match &cfg.resize {
            Some(crate::config::ResizeConfig::Width { width }) => ResizeMode::Width(*width),
            Some(crate::config::ResizeConfig::Scale { scale }) => ResizeMode::Scale(*scale),
            None => ResizeMode::None,
        };

        // Heuristic
        self.state.config.heuristic = match cfg.heuristic.as_str() {
            "best-short-side-fit" => PackingHeuristic::BestShortSideFit,
            "best-long-side-fit" => PackingHeuristic::BestLongSideFit,
            "best-area-fit" => PackingHeuristic::BestAreaFit,
            "bottom-left" => PackingHeuristic::BottomLeft,
            "contact-point" => PackingHeuristic::ContactPoint,
            "best" => PackingHeuristic::Best,
            unknown => {
                self.state.runtime.status = Status::Done {
                    result: StatusResult::Error(format!(
                        "Unknown heuristic '{}' in config. Valid: best-short-side-fit, \
                         best-long-side-fit, best-area-fit, bottom-left, contact-point, best",
                        unknown
                    )),
                    at: std::time::Instant::now(),
                };
                return;
            }
        };

        // Pack mode
        self.state.config.pack_mode = match cfg.pack_mode.as_str() {
            "single" => PackMode::Single,
            "best" => PackMode::Best,
            unknown => {
                self.state.runtime.status = Status::Done {
                    result: StatusResult::Error(format!(
                        "Unknown pack_mode '{}' in config. Valid: single, best",
                        unknown
                    )),
                    at: std::time::Instant::now(),
                };
                return;
            }
        };

        // Compress
        self.state.config.compress = cfg.compress.as_ref().map(|c| match c {
            crate::config::CompressConfig::Level(n) => CompressionLevel::Level(*n),
            crate::config::CompressConfig::Max(_) => CompressionLevel::Max,
        });

        self.state.config.opaque = cfg.opaque;

        // Set config path and save hash
        self.state.runtime.config_path = Some(config_path);
        self.state.runtime.last_saved_config_hash = Some(self.state.config.full_config_hash());

        // Clear thumbnails and trigger repack
        self.state.runtime.thumbnails.clear();
        self.state.runtime.last_packed_hash = None;
    }

    fn save_current_config(&mut self) -> Result<(), String> {
        let Some(path) = &self.state.runtime.config_path else {
            return Err("No config file path set".to_string());
        };

        let bento_config = self.config_to_bento_config(path);
        save_config(&bento_config, path).map_err(|e| e.to_string())?;

        self.state.runtime.last_saved_config_hash = Some(self.state.config.full_config_hash());
        Ok(())
    }

    fn config_to_bento_config(&self, config_path: &std::path::Path) -> BentoConfig {
        use crate::config::{CompressConfig, ResizeConfig as CfgResize};

        let config_dir = config_path.parent().unwrap_or(std::path::Path::new("."));

        BentoConfig {
            version: 1,
            input: self
                .state
                .config
                .input_paths
                .iter()
                .map(|p| crate::config::make_relative(p, config_dir))
                .collect(),
            output_dir: crate::config::make_relative(&self.state.config.output_dir, config_dir),
            name: self.state.config.name.clone(),
            format: Some(match self.state.config.format {
                OutputFormat::Json => "json".to_string(),
                OutputFormat::Godot => "godot".to_string(),
                OutputFormat::Tpsheet => "tpsheet".to_string(),
            }),
            max_width: self.state.config.max_width,
            max_height: self.state.config.max_height,
            padding: self.state.config.padding,
            pot: self.state.config.pot,
            trim: self.state.config.trim,
            trim_margin: self.state.config.trim_margin,
            extrude: self.state.config.extrude,
            resize: match self.state.config.resize_mode {
                ResizeMode::None => None,
                ResizeMode::Width(w) => Some(CfgResize::Width { width: w }),
                ResizeMode::Scale(s) => Some(CfgResize::Scale { scale: s }),
            },
            heuristic: match self.state.config.heuristic {
                PackingHeuristic::BestShortSideFit => "best-short-side-fit".to_string(),
                PackingHeuristic::BestLongSideFit => "best-long-side-fit".to_string(),
                PackingHeuristic::BestAreaFit => "best-area-fit".to_string(),
                PackingHeuristic::BottomLeft => "bottom-left".to_string(),
                PackingHeuristic::ContactPoint => "contact-point".to_string(),
                PackingHeuristic::Best => "best".to_string(),
            },
            pack_mode: match self.state.config.pack_mode {
                PackMode::Single => "single".to_string(),
                PackMode::Best => "best".to_string(),
            },
            compress: self.state.config.compress.map(|c| match c {
                CompressionLevel::Level(n) => CompressConfig::Level(n),
                CompressionLevel::Max => CompressConfig::Max("max".to_string()),
            }),
            opaque: self.state.config.opaque,
        }
    }

    pub fn new_project(&mut self) {
        self.state.config = AppConfig::default();
        self.state.runtime.config_path = None;
        self.state.runtime.last_saved_config_hash = None;
        self.state.runtime.atlases = None;
        self.state.runtime.atlas_textures.clear();
        self.state.runtime.thumbnails.clear();
        self.state.runtime.last_packed_hash = None;
    }

    /// Execute a pending action (after unsaved changes confirmation)
    fn execute_pending_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::NewProject => self.new_project(),
            PendingAction::OpenConfig(path) => self.load_config_file(&path),
            PendingAction::CloseWindow => {
                self.allowed_to_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Check if there are unsaved changes and show dialog if needed.
    /// Returns true if the action can proceed immediately (no unsaved changes),
    /// false if a dialog was shown (action is deferred).
    fn check_unsaved_changes(&mut self, action: PendingAction) -> bool {
        if self.state.runtime.is_config_dirty(&self.state.config) {
            self.unsaved_changes_dialog = Some(UnsavedChangesDialog::new(action));
            false
        } else {
            true
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
                    // Update hash to prevent auto-repack retry with same failing config
                    self.state.runtime.last_packed_hash =
                        Some(self.state.config.pack_settings_hash());
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

                    // Auto-save config if we have a config path
                    if self.state.runtime.config_path.is_some() {
                        if let Err(e) = self.save_current_config() {
                            // Log error but don't fail the export
                            log::warn!("Failed to auto-save config: {}", e);
                        }
                    }
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

        // Only start new estimation if we have atlases and no estimation is running
        let Some(atlases) = &self.state.runtime.atlases else {
            return;
        };

        if self.state.runtime.size_estimate_task.is_some() {
            return;
        }

        // Spawn background thread to re-estimate PNG sizes
        let atlases = atlases.clone();
        let opaque = self.state.config.opaque;
        let compress = self.state.config.compress;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let sizes: Vec<usize> = atlases
                .iter()
                .map(|a| estimate_png_size(&a.image, opaque, compress))
                .collect();
            let _ = tx.send(Ok(sizes));
        });

        self.state.runtime.size_estimate_task = Some(BackgroundTask::new(rx));
        self.state.runtime.last_export_hash = Some(current_export_hash);
    }

    /// Poll background size estimation task for completion
    fn poll_size_estimate_task(&mut self) {
        if let Some(task) = &self.state.runtime.size_estimate_task
            && let Some(result) = task.poll()
        {
            self.state.runtime.size_estimate_task = None;
            if let Ok(sizes) = result {
                self.state.runtime.atlas_png_sizes = sizes;
            }
        }
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
        self.state
            .runtime
            .thumbnails
            .retain(|path, _| self.state.config.input_paths.contains(path));
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

    // Load sprites (check cancellation during load)
    let sprites = load_sprites(
        &config.input_paths,
        config.trim,
        config.trim_margin,
        resize_width,
        resize_scale,
        Some(&cancel_token),
    )
    .map_err(|e| e.to_string())?;

    // Build atlas
    let atlases = AtlasBuilder::new(config.max_width, config.max_height)
        .padding(config.padding)
        .heuristic(config.heuristic)
        .power_of_two(config.pot)
        .extrude(config.extrude)
        .pack_mode(config.pack_mode)
        .cancel_token(cancel_token.clone())
        .build(sprites)
        .map_err(|e| e.to_string())?;

    // Estimate PNG sizes on background thread (check cancellation)
    let mut png_sizes = Vec::with_capacity(atlases.len());
    for atlas in &atlases {
        if cancel_token.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        png_sizes.push(estimate_png_size(
            &atlas.image,
            config.opaque,
            config.compress,
        ));
    }

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
        OutputFormat::Tpsheet => {
            write_tpsheet(atlases, &config.output_dir, &config.name).map_err(|e| e.to_string())?;
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
        eframe::set_value(
            storage,
            LAST_INPUT_DIR_KEY,
            &self.state.runtime.last_input_dir,
        );
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update window title
        let title = if let Some(path) = &self.state.runtime.config_path {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());
            let dirty = if self.state.runtime.is_config_dirty(&self.state.config) {
                " *"
            } else {
                ""
            };
            format!("Bento - {}{}", name, dirty)
        } else {
            "Bento".to_string()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Handle window close request
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.allowed_to_close {
                // User already confirmed, allow close
            } else if self.state.runtime.is_config_dirty(&self.state.config) {
                // Has unsaved changes, show confirmation dialog
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.unsaved_changes_dialog = Some(UnsavedChangesDialog::new(PendingAction::CloseWindow));
            }
            // If not dirty, allow the close to proceed naturally
        }

        // Handle config chooser dialog
        if let Some(ref mut chooser) = self.config_chooser {
            if let Some(selected) = chooser.show(ctx) {
                self.config_chooser = None;
                if !selected.as_os_str().is_empty() {
                    self.load_config_file(&selected);
                }
            }
        }

        // Handle unsaved changes dialog
        if let Some(ref mut dialog) = self.unsaved_changes_dialog {
            if let Some(choice) = dialog.show(ctx) {
                let pending_action = dialog.pending_action.clone();
                self.unsaved_changes_dialog = None;

                match choice {
                    UnsavedChangesChoice::Cancel => {
                        // Do nothing, user cancelled
                    }
                    UnsavedChangesChoice::DontSave => {
                        // Proceed without saving
                        self.execute_pending_action(pending_action, ctx);
                    }
                    UnsavedChangesChoice::Save => {
                        // If no path, prompt for Save As first
                        if self.state.runtime.config_path.is_none() {
                            let dialog = rfd::FileDialog::new()
                                .add_filter("Bento Config", &["bento"])
                                .set_file_name("atlas.bento");
                            if let Some(path) = dialog.save_file() {
                                let path = if path.extension().is_some_and(|e| e == "bento") {
                                    path
                                } else {
                                    path.with_extension("bento")
                                };
                                self.state.runtime.config_path = Some(path);
                            }
                        }
                        // Now try to save (if we have a path)
                        if self.state.runtime.config_path.is_some()
                            && self.save_current_config().is_ok()
                        {
                            self.execute_pending_action(pending_action, ctx);
                        }
                        // If still no path (user cancelled Save As), don't proceed
                    }
                }
            }
        }

        // Handle dropped files
        self.handle_dropped_files(ctx);

        // Poll background tasks
        self.poll_pack_task(ctx);
        self.poll_export_task();
        self.poll_size_estimate_task();

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
            || self.state.runtime.size_estimate_task.is_some()
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
                let action = panels::input_panel(ui, &mut self.state);

                if action.new_project && self.check_unsaved_changes(PendingAction::NewProject) {
                    self.new_project();
                }

                if let Some(path) = action.open_config_path {
                    if action.save_config_as {
                        // Save As: set path and save (no unsaved changes check needed)
                        self.state.runtime.config_path = Some(path.clone());
                        if let Err(e) = self.save_current_config() {
                            self.state.runtime.status = Status::Done {
                                result: StatusResult::Error(format!("Failed to save: {}", e)),
                                at: std::time::Instant::now(),
                            };
                        }
                    } else if self.check_unsaved_changes(PendingAction::OpenConfig(path.clone())) {
                        // Open: load the config (if no unsaved changes or user confirmed)
                        self.load_config_file(&path);
                    }
                } else if action.save_config {
                    if let Err(e) = self.save_current_config() {
                        self.state.runtime.status = Status::Done {
                            result: StatusResult::Error(format!("Failed to save: {}", e)),
                            at: std::time::Instant::now(),
                        };
                    }
                }
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
