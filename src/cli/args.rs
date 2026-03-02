use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "bento")]
#[command(version, about = "Sprite atlas packer", long_about = None)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Output JSON metadata (recommended for Godot)
    Json(CommonArgs),
    /// Output individual Godot .tres files
    Godot(CommonArgs),
    /// Output TexturePacker .tpsheet metadata
    Tpsheet(CommonArgs),
    /// Launch the GUI
    #[cfg(feature = "gui")]
    Gui,
}

#[derive(Args, Debug, Clone)]
pub struct CommonArgs {
    /// Input image files
    #[arg(required_unless_present = "config")]
    pub input: Vec<PathBuf>,

    /// Load settings from a .bento config file
    #[arg(short = 'c', long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Output directory for atlas files [default: .]
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Base name for output files (atlas_0.png, atlas.json, etc.) [default: atlas]
    #[arg(short = 'n', long)]
    pub name: Option<String>,

    /// Maximum atlas width in pixels [default: 4096]
    #[arg(long)]
    pub max_width: Option<u32>,

    /// Maximum atlas height in pixels [default: 4096]
    #[arg(long)]
    pub max_height: Option<u32>,

    /// Padding between sprites in pixels [default: 1]
    #[arg(short, long)]
    pub padding: Option<u32>,

    /// Disable sprite trimming (remove transparent borders)
    #[arg(long)]
    pub no_trim: bool,

    /// Keep N pixels of transparent border after trimming [default: 0]
    #[arg(long)]
    pub trim_margin: Option<u32>,

    /// Packing heuristic to use [default: best-short-side-fit]
    #[arg(long, value_enum)]
    pub heuristic: Option<PackingHeuristic>,

    /// Output RGB instead of RGBA (opaque atlas)
    #[arg(long)]
    pub opaque: bool,

    /// Force power-of-two atlas dimensions
    #[arg(long)]
    pub pot: bool,

    /// Extrude sprite edges by N pixels (helps with texture bleeding) [default: 0]
    #[arg(long)]
    pub extrude: Option<u32>,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Resize images to target width in pixels (preserves aspect ratio)
    #[arg(long, value_name = "PIXELS", conflicts_with = "resize_scale")]
    pub resize_width: Option<u32>,

    /// Resize images by scale factor (e.g., 0.5 for half size)
    #[arg(long, value_name = "FACTOR", conflicts_with = "resize_width")]
    pub resize_scale: Option<f32>,

    /// Resize filter algorithm [default: lanczos3]
    #[arg(long, value_enum)]
    pub resize_filter: Option<ResizeFilter>,

    /// Pack mode: single (use one ordering) or best (try multiple orderings) [default: single]
    #[arg(long, value_enum)]
    pub pack_mode: Option<PackMode>,

    /// Use only the filename (no directory prefix) in sprite names
    #[arg(long)]
    pub filename_only: bool,

    /// Compress PNG output (0-6 or 'max'). Default level is 2 if flag is present without value.
    #[arg(long, value_name = "LEVEL", default_missing_value = "2", num_args = 0..=1)]
    pub compress: Option<CompressionLevel>,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub enum PackMode {
    /// Use sprites in input order
    #[default]
    Single,
    /// Try multiple sprite orderings and pick the best result
    Best,
}

/// Resize filter algorithm
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq, Hash)]
pub enum ResizeFilter {
    /// Nearest neighbor (best for pixel art)
    #[value(name = "nearest")]
    Nearest,
    /// Bilinear interpolation
    #[value(name = "triangle")]
    Triangle,
    /// Cubic interpolation (bicubic)
    #[value(name = "catmull-rom", alias = "bicubic")]
    CatmullRom,
    /// Gaussian filter
    #[value(name = "gaussian")]
    Gaussian,
    /// Lanczos with window 3 (highest quality)
    #[default]
    #[value(name = "lanczos3")]
    Lanczos3,
}

impl ResizeFilter {
    pub fn to_image_filter(self) -> image::imageops::FilterType {
        match self {
            ResizeFilter::Nearest => image::imageops::FilterType::Nearest,
            ResizeFilter::Triangle => image::imageops::FilterType::Triangle,
            ResizeFilter::CatmullRom => image::imageops::FilterType::CatmullRom,
            ResizeFilter::Gaussian => image::imageops::FilterType::Gaussian,
            ResizeFilter::Lanczos3 => image::imageops::FilterType::Lanczos3,
        }
    }
}

/// PNG compression level (0-6 or max)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// Optimization level 0-6
    Level(u8),
    /// Maximum compression
    Max,
}

impl std::str::FromStr for CompressionLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("max") {
            Ok(CompressionLevel::Max)
        } else {
            s.parse::<u8>()
                .map_err(|_e| format!("invalid compression level: {}", s))
                .and_then(|n| {
                    if n <= 6 {
                        Ok(CompressionLevel::Level(n))
                    } else {
                        Err(format!("compression level must be 0-6 or 'max', got {}", n))
                    }
                })
        }
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        CompressionLevel::Level(2)
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub enum PackingHeuristic {
    /// Best Short Side Fit - minimizes the shorter leftover side
    #[default]
    #[value(name = "best-short-side-fit")]
    BestShortSideFit,
    /// Best Long Side Fit - minimizes the longer leftover side
    #[value(name = "best-long-side-fit")]
    BestLongSideFit,
    /// Best Area Fit - picks the smallest free rectangle
    #[value(name = "best-area-fit")]
    BestAreaFit,
    /// Bottom Left - Tetris-style packing
    #[value(name = "bottom-left")]
    BottomLeft,
    /// Contact Point - maximizes contact with placed rectangles and bin edges
    #[value(name = "contact-point")]
    ContactPoint,
    /// Best - tries all heuristics and picks the most efficient result
    #[value(name = "best")]
    Best,
}
