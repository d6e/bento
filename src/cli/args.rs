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
    #[arg(required = true)]
    pub input: Vec<PathBuf>,

    /// Output directory for atlas files
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Base name for output files (atlas_0.png, atlas.json, etc.)
    #[arg(short = 'n', long, default_value = "atlas")]
    pub name: String,

    /// Maximum atlas width in pixels
    #[arg(long, default_value = "4096")]
    pub max_width: u32,

    /// Maximum atlas height in pixels
    #[arg(long, default_value = "4096")]
    pub max_height: u32,

    /// Padding between sprites in pixels
    #[arg(short, long, default_value = "1")]
    pub padding: u32,

    /// Disable sprite trimming (remove transparent borders)
    #[arg(long)]
    pub no_trim: bool,

    /// Keep N pixels of transparent border after trimming
    #[arg(long, default_value = "0")]
    pub trim_margin: u32,

    /// Packing heuristic to use
    #[arg(long, value_enum, default_value = "best-short-side-fit")]
    pub heuristic: PackingHeuristic,

    /// Output RGB instead of RGBA (opaque atlas)
    #[arg(long)]
    pub opaque: bool,

    /// Force power-of-two atlas dimensions
    #[arg(long)]
    pub pot: bool,

    /// Extrude sprite edges by N pixels (helps with texture bleeding)
    #[arg(long, default_value = "0")]
    pub extrude: u32,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Resize images to target width in pixels (preserves aspect ratio)
    #[arg(long, value_name = "PIXELS", conflicts_with = "resize_scale")]
    pub resize_width: Option<u32>,

    /// Resize images by scale factor (e.g., 0.5 for half size)
    #[arg(long, value_name = "FACTOR", conflicts_with = "resize_width")]
    pub resize_scale: Option<f32>,

    /// Pack mode: single (use one ordering) or best (try multiple orderings)
    #[arg(long, value_enum, default_value = "single")]
    pub pack_mode: PackMode,

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
