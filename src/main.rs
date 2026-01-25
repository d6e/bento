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

#[allow(clippy::print_stderr)]
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

    // Determine output directory: CLI > config > default
    let output = args.output.clone().unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.resolve_output_dir())
            .unwrap_or_else(|| PathBuf::from("."))
    });

    // Determine name: CLI > config > default
    let name = args.name.clone().unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.name.clone())
            .unwrap_or_else(|| "atlas".to_string())
    });

    // For numeric fields: CLI > config > default
    let max_width = args.max_width.unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.max_width)
            .unwrap_or(4096)
    });

    let max_height = args.max_height.unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.max_height)
            .unwrap_or(4096)
    });

    let padding = args.padding.unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.padding)
            .unwrap_or(1)
    });

    let trim_margin = args.trim_margin.unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.trim_margin)
            .unwrap_or(0)
    });

    let extrude = args.extrude.unwrap_or_else(|| {
        loaded_config
            .as_ref()
            .map(|lc| lc.config.extrude)
            .unwrap_or(0)
    });

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

    // Heuristic: CLI > config > default
    let heuristic = if let Some(h) = args.heuristic {
        h
    } else if let Some(ref lc) = loaded_config {
        parse_heuristic(&lc.config.heuristic).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown heuristic '{}' in config file. Valid values: best-short-side-fit, \
                 best-long-side-fit, best-area-fit, bottom-left, contact-point, best",
                lc.config.heuristic
            )
        })?
    } else {
        PackingHeuristic::BestShortSideFit
    };

    // Pack mode: CLI > config > default
    let pack_mode = if let Some(m) = args.pack_mode {
        m
    } else if let Some(ref lc) = loaded_config {
        parse_pack_mode(&lc.config.pack_mode).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown pack_mode '{}' in config file. Valid values: single, best",
                lc.config.pack_mode
            )
        })?
    } else {
        PackMode::Single
    };

    // Resize: CLI options override config
    let (resize_width, resize_scale) = if args.resize_width.is_some() || args.resize_scale.is_some()
    {
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
