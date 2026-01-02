use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::atlas::Atlas;
use crate::cli::{CompressionLevel, PackMode, PackingHeuristic};

// ─────────────────────────────────────────────────────────────────────────────
// GUI-specific enums
// ─────────────────────────────────────────────────────────────────────────────

/// Output format selection (mirrors CLI subcommands)
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Json,
    Godot,
}

/// Resize mode (mirrors CLI's mutually exclusive resize options)
#[derive(Default, Clone, Copy, PartialEq)]
pub enum ResizeMode {
    #[default]
    None,
    Width(u32),
    Scale(f32),
}

// ─────────────────────────────────────────────────────────────────────────────
// Background Task Abstraction
// ─────────────────────────────────────────────────────────────────────────────

/// Generic handle for background operations (packing, exporting)
pub struct BackgroundTask<T> {
    receiver: mpsc::Receiver<Result<T, String>>,
}

impl<T> BackgroundTask<T> {
    pub fn new(receiver: mpsc::Receiver<Result<T, String>>) -> Self {
        Self { receiver }
    }

    /// Non-blocking poll for result
    pub fn poll(&self) -> Option<Result<T, String>> {
        self.receiver.try_recv().ok()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// State: Split into Config (serializable) and Runtime (transient)
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level application state
#[derive(Default)]
pub struct AppState {
    pub config: AppConfig,
    pub runtime: RuntimeState,
}

/// Serializable configuration (settings + input/output paths)
/// Can be cloned cheaply for worker threads
#[derive(Clone)]
pub struct AppConfig {
    // Input
    pub input_paths: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub name: String,
    pub format: OutputFormat,

    // Pack settings (affect atlas output)
    pub max_width: u32,
    pub max_height: u32,
    pub padding: u32,
    pub pot: bool,
    pub trim: bool,
    pub trim_margin: u32,
    pub extrude: u32,
    pub resize_mode: ResizeMode,
    pub heuristic: PackingHeuristic,
    pub pack_mode: PackMode,

    // Export settings (only affect file output, not packing)
    pub compress: Option<CompressionLevel>,
    pub opaque: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            input_paths: Vec::new(),
            output_dir: PathBuf::from("."),
            name: "atlas".to_string(),
            format: OutputFormat::default(),

            max_width: 4096,
            max_height: 4096,
            padding: 1,
            pot: false,
            trim: true,
            trim_margin: 0,
            extrude: 0,
            resize_mode: ResizeMode::default(),
            heuristic: PackingHeuristic::default(),
            pack_mode: PackMode::default(),

            compress: None,
            opaque: false,
        }
    }
}

impl AppConfig {
    /// Hash of settings that affect packing output (not export settings)
    /// Used for change detection to trigger auto-repack
    pub fn pack_settings_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.input_paths.hash(&mut hasher);
        self.max_width.hash(&mut hasher);
        self.max_height.hash(&mut hasher);
        self.padding.hash(&mut hasher);
        self.pot.hash(&mut hasher);
        self.trim.hash(&mut hasher);
        self.trim_margin.hash(&mut hasher);
        self.extrude.hash(&mut hasher);
        std::mem::discriminant(&self.resize_mode).hash(&mut hasher);
        std::mem::discriminant(&self.heuristic).hash(&mut hasher);
        std::mem::discriminant(&self.pack_mode).hash(&mut hasher);
        hasher.finish()
    }
}

/// Transient runtime state (not serializable)
pub struct RuntimeState {
    // Packed atlas data
    pub atlases: Option<Arc<Vec<Atlas>>>,
    pub selected_atlas: usize,

    // Texture handles for preview (one per atlas)
    pub atlas_textures: Vec<egui::TextureHandle>,

    // Preview controls
    pub preview_zoom: f32,
    pub preview_offset: egui::Vec2,

    // Status and tasks
    pub status: Status,
    pub pack_task: Option<BackgroundTask<Arc<Vec<Atlas>>>>,
    pub export_task: Option<BackgroundTask<()>>,

    // Auto-repack tracking
    pub auto_repack: bool,
    pub last_packed_hash: Option<u64>,
    pub pending_repack_at: Option<Instant>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            atlases: None,
            selected_atlas: 0,

            atlas_textures: Vec::new(),
            preview_zoom: 1.0,
            preview_offset: egui::Vec2::ZERO,

            status: Status::Idle,
            pack_task: None,
            export_task: None,

            auto_repack: false,
            last_packed_hash: None,
            pending_repack_at: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Status with timing support
// ─────────────────────────────────────────────────────────────────────────────

pub enum Status {
    Idle,
    Working {
        operation: Operation,
        started_at: Instant,
    },
    Done {
        result: StatusResult,
        at: Instant,
    },
}

#[derive(Clone, Copy)]
pub enum Operation {
    Packing,
    Exporting,
}

pub enum StatusResult {
    Success(String),
    Error(String),
}

impl Status {
    /// Auto-clear old success messages, keep errors visible
    pub fn maybe_clear(&mut self, max_age: Duration) {
        if let Status::Done {
            result: StatusResult::Success(_),
            at,
        } = self
            && at.elapsed() > max_age
        {
            *self = Status::Idle;
        }
    }
}
