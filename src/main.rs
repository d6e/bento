use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use log::info;

use bento::atlas::AtlasBuilder;
use bento::cli::{CliArgs, Command, CommonArgs, CompressionLevel, PackMode, PackingHeuristic};
use bento::config::{CompressConfig, LoadedConfig, ResizeConfig};
use bento::output::{save_atlas_image, write_godot_resources, write_json, write_tpsheet};
use bento::sprite::load_sprites;

fn main() {
    if let Err(e) = run() {
        // Use eprintln instead of error! because logger may not be initialized
        // (e.g., config loading fails before logger init)
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Launch GUI if no arguments provided and gui feature is enabled
    #[cfg(feature = "gui")]
    if std::env::args().len() == 1 {
        return bento::gui::run(None);
    }

    let cli = CliArgs::parse();

    // Handle GUI command
    #[cfg(feature = "gui")]
    if matches!(cli.command, Command::Gui) {
        return bento::gui::run(None);
    }

    // Extract common args from subcommand
    let args = match &cli.command {
        Command::Json(args) | Command::Godot(args) | Command::Tpsheet(args) => args.clone(),
        #[cfg(feature = "gui")]
        Command::Gui => unreachable!(),
    };

    // Load config if specified and merge with CLI args
    let merged = merge_config_with_args(&args)?;

    // Initialize logging
    env_logger::Builder::new()
        .filter_level(if merged.verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .format_timestamp(None)
        .format_target(false)
        .init();

    info!("Bento texture packer v{}", env!("CARGO_PKG_VERSION"));

    // Create output directory if it doesn't exist
    if !merged.output.exists() {
        fs::create_dir_all(&merged.output)?;
    }

    // Load sprites
    let sprites = load_sprites(
        &merged.input,
        merged.trim,
        merged.trim_margin,
        merged.resize_width,
        merged.resize_scale,
        None, // No cancellation for CLI
    )?;
    info!("Loaded {} sprites", sprites.len());

    // Build atlases
    let atlases = AtlasBuilder::new(merged.max_width, merged.max_height)
        .padding(merged.padding)
        .heuristic(merged.heuristic)
        .power_of_two(merged.pot)
        .extrude(merged.extrude)
        .pack_mode(merged.pack_mode)
        .build(sprites)?;

    // Save atlas images
    for atlas in &atlases {
        let path = merged
            .output
            .join(format!("{}_{}.png", merged.name, atlas.index));
        save_atlas_image(atlas, &path, merged.opaque, merged.compress)?;
        info!("Saved {}", path.display());
    }

    // Write format-specific output
    match &cli.command {
        Command::Json(_) => {
            write_json(&atlases, &merged.output, &merged.name)?;
            info!("Generated {}.json", merged.name);
        }
        Command::Godot(_) => {
            write_godot_resources(&atlases, &merged.output, &merged.name, None)?;
            info!(
                "Generated {} Godot .tres files",
                atlases.iter().map(|a| a.sprites.len()).sum::<usize>()
            );
        }
        Command::Tpsheet(_) => {
            write_tpsheet(&atlases, &merged.output, &merged.name)?;
            info!("Generated {}.tpsheet", merged.name);
        }
        #[cfg(feature = "gui")]
        Command::Gui => unreachable!(),
    }

    info!("Done!");

    Ok(())
}

/// Merged configuration from CLI args and optional config file.
struct MergedConfig {
    input: Vec<PathBuf>,
    output: PathBuf,
    name: String,
    max_width: u32,
    max_height: u32,
    padding: u32,
    trim: bool,
    trim_margin: u32,
    heuristic: PackingHeuristic,
    opaque: bool,
    pot: bool,
    extrude: u32,
    verbose: bool,
    resize_width: Option<u32>,
    resize_scale: Option<f32>,
    pack_mode: PackMode,
    compress: Option<CompressionLevel>,
}

/// Merge config file values with CLI arguments.
/// CLI arguments always take precedence over config values.
fn merge_config_with_args(args: &CommonArgs) -> Result<MergedConfig> {
    // Load config if specified
    let loaded_config = if let Some(config_path) = &args.config {
        Some(
            LoadedConfig::load(config_path)
                .with_context(|| format!("failed to load config: {}", config_path.display()))?,
        )
    } else {
        None
    };

    // Determine input files: CLI args override config
    let input = if !args.input.is_empty() {
        args.input.clone()
    } else if let Some(ref lc) = loaded_config {
        lc.resolve_inputs()
            .context("failed to resolve input files from config")?
    } else {
        // This shouldn't happen due to clap's required_unless_present
        Vec::new()
    };

    // Determine output directory: CLI overrides config
    // CLI default is ".", so we check if config provides a different value
    let output = if let Some(ref lc) = loaded_config {
        if args.output == PathBuf::from(".") {
            lc.resolve_output_dir()
        } else {
            args.output.clone()
        }
    } else {
        args.output.clone()
    };

    // Determine name: CLI overrides config
    let name = if let Some(ref lc) = loaded_config {
        if args.name == "atlas" {
            lc.config.name.clone()
        } else {
            args.name.clone()
        }
    } else {
        args.name.clone()
    };

    // For numeric fields with defaults, CLI wins if different from default
    let (max_width, max_height) = if let Some(ref lc) = loaded_config {
        (
            if args.max_width == 4096 {
                lc.config.max_width
            } else {
                args.max_width
            },
            if args.max_height == 4096 {
                lc.config.max_height
            } else {
                args.max_height
            },
        )
    } else {
        (args.max_width, args.max_height)
    };

    let padding = if let Some(ref lc) = loaded_config {
        if args.padding == 1 {
            lc.config.padding
        } else {
            args.padding
        }
    } else {
        args.padding
    };

    let trim_margin = if let Some(ref lc) = loaded_config {
        if args.trim_margin == 0 {
            lc.config.trim_margin
        } else {
            args.trim_margin
        }
    } else {
        args.trim_margin
    };

    let extrude = if let Some(ref lc) = loaded_config {
        if args.extrude == 0 {
            lc.config.extrude
        } else {
            args.extrude
        }
    } else {
        args.extrude
    };

    // Boolean flags: CLI presence sets them to true, otherwise use config
    let trim = if args.no_trim {
        false
    } else if let Some(ref lc) = loaded_config {
        lc.config.trim
    } else {
        true // default is to trim
    };

    let pot = if args.pot {
        true
    } else if let Some(ref lc) = loaded_config {
        lc.config.pot
    } else {
        false
    };

    let opaque = if args.opaque {
        true
    } else if let Some(ref lc) = loaded_config {
        lc.config.opaque
    } else {
        false
    };

    // Verbose is CLI-only
    let verbose = args.verbose;

    // Heuristic: parse from config if CLI is default
    let heuristic = if let Some(ref lc) = loaded_config {
        if args.heuristic == PackingHeuristic::BestShortSideFit {
            parse_heuristic(&lc.config.heuristic).unwrap_or(args.heuristic)
        } else {
            args.heuristic
        }
    } else {
        args.heuristic
    };

    // Pack mode: parse from config if CLI is default
    let pack_mode = if let Some(ref lc) = loaded_config {
        if args.pack_mode == PackMode::Single {
            parse_pack_mode(&lc.config.pack_mode).unwrap_or(args.pack_mode)
        } else {
            args.pack_mode
        }
    } else {
        args.pack_mode
    };

    // Resize: CLI options override config
    let (resize_width, resize_scale) =
        if args.resize_width.is_some() || args.resize_scale.is_some() {
            (args.resize_width, args.resize_scale)
        } else if let Some(ref lc) = loaded_config {
            match &lc.config.resize {
                Some(ResizeConfig::Width { width }) => (Some(*width), None),
                Some(ResizeConfig::Scale { scale }) => (None, Some(*scale)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

    // Compress: CLI option overrides config
    let compress = if args.compress.is_some() {
        args.compress
    } else if let Some(ref lc) = loaded_config {
        lc.config.compress.as_ref().map(|c| match c {
            CompressConfig::Level(n) => CompressionLevel::Level(*n),
            CompressConfig::Max(_) => CompressionLevel::Max,
        })
    } else {
        None
    };

    Ok(MergedConfig {
        input,
        output,
        name,
        max_width,
        max_height,
        padding,
        trim,
        trim_margin,
        heuristic,
        opaque,
        pot,
        extrude,
        verbose,
        resize_width,
        resize_scale,
        pack_mode,
        compress,
    })
}

fn parse_heuristic(s: &str) -> Option<PackingHeuristic> {
    match s {
        "best-short-side-fit" => Some(PackingHeuristic::BestShortSideFit),
        "best-long-side-fit" => Some(PackingHeuristic::BestLongSideFit),
        "best-area-fit" => Some(PackingHeuristic::BestAreaFit),
        "bottom-left" => Some(PackingHeuristic::BottomLeft),
        "contact-point" => Some(PackingHeuristic::ContactPoint),
        "best" => Some(PackingHeuristic::Best),
        _ => None,
    }
}

fn parse_pack_mode(s: &str) -> Option<PackMode> {
    match s {
        "single" => Some(PackMode::Single),
        "best" => Some(PackMode::Best),
        _ => None,
    }
}
