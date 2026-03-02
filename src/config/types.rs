use serde::{Deserialize, Serialize};

/// Configuration for resizing sprites.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResizeConfig {
    /// Resize to a specific width in pixels (preserves aspect ratio)
    Width { width: u32 },
    /// Resize by a scale factor (e.g., 0.5 for half size)
    Scale { scale: f32 },
}

/// PNG compression level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompressConfig {
    /// Optimization level 0-6
    Level(u8),
    /// Maximum compression ("max")
    Max(String),
}

/// Bento configuration file structure.
///
/// All paths in the config are relative to the config file location.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BentoConfig {
    /// Config file version (currently 1)
    pub version: u32,
    /// Input file paths or glob patterns
    pub input: Vec<String>,
    /// Output directory for atlas files
    pub output_dir: String,
    /// Base name for output files (atlas_0.png, atlas.json, etc.)
    pub name: String,
    /// Output format: "json", "godot", or "tpsheet"
    pub format: Option<String>,
    /// Maximum atlas width in pixels
    pub max_width: u32,
    /// Maximum atlas height in pixels
    pub max_height: u32,
    /// Padding between sprites in pixels
    pub padding: u32,
    /// Force power-of-two atlas dimensions
    pub pot: bool,
    /// Enable sprite trimming (remove transparent borders)
    pub trim: bool,
    /// Keep N pixels of transparent border after trimming
    pub trim_margin: u32,
    /// Extrude sprite edges by N pixels (helps with texture bleeding)
    pub extrude: u32,
    /// Resize configuration (optional)
    pub resize: Option<ResizeConfig>,
    /// Resize filter algorithm (nearest, triangle, catmull-rom, gaussian, lanczos3)
    pub resize_filter: String,
    /// Packing heuristic to use
    pub heuristic: String,
    /// Pack mode: "single" or "best"
    pub pack_mode: String,
    /// PNG compression configuration (optional)
    pub compress: Option<CompressConfig>,
    /// Output RGB instead of RGBA (opaque atlas)
    pub opaque: bool,
    /// Use only the filename (no directory prefix) in sprite names
    pub filename_only: bool,
}

impl Default for BentoConfig {
    fn default() -> Self {
        Self {
            version: 1,
            input: Vec::new(),
            output_dir: ".".to_string(),
            name: "atlas".to_string(),
            format: None,
            max_width: 4096,
            max_height: 4096,
            padding: 1,
            pot: false,
            trim: true,
            trim_margin: 0,
            extrude: 0,
            resize: None,
            resize_filter: "lanczos3".to_string(),
            heuristic: "best-short-side-fit".to_string(),
            pack_mode: "single".to_string(),
            compress: None,
            opaque: false,
            filename_only: false,
        }
    }
}
