use std::fs;

use anyhow::Result;
use clap::Parser;
use log::{error, info};

use bento::atlas::AtlasBuilder;
use bento::cli::{CliArgs, Command};
use bento::output::{save_atlas_image, write_godot_resources, write_json};
use bento::sprite::load_sprites;

fn main() {
    if let Err(e) = run() {
        error!("{:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Launch GUI if no arguments provided and gui feature is enabled
    #[cfg(feature = "gui")]
    if std::env::args().len() == 1 {
        return bento::gui::run();
    }

    let cli = CliArgs::parse();

    // Handle GUI command
    #[cfg(feature = "gui")]
    if matches!(cli.command, Command::Gui) {
        return bento::gui::run();
    }

    // Extract common args from subcommand
    let args = match &cli.command {
        Command::Json(args) | Command::Godot(args) => args,
        #[cfg(feature = "gui")]
        Command::Gui => unreachable!(),
    };

    // Initialize logging
    env_logger::Builder::new()
        .filter_level(if args.verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .format_timestamp(None)
        .format_target(false)
        .init();

    info!("Bento texture packer v{}", env!("CARGO_PKG_VERSION"));

    // Create output directory if it doesn't exist
    if !args.output.exists() {
        fs::create_dir_all(&args.output)?;
    }

    // Load sprites
    let sprites = load_sprites(
        &args.input,
        !args.no_trim,
        args.trim_margin,
        args.resize_width,
        args.resize_scale,
        None, // No cancellation for CLI
    )?;
    info!("Loaded {} sprites", sprites.len());

    // Build atlases
    let atlases = AtlasBuilder::new(args.max_width, args.max_height)
        .padding(args.padding)
        .heuristic(args.heuristic)
        .power_of_two(args.pot)
        .extrude(args.extrude)
        .pack_mode(args.pack_mode)
        .build(sprites)?;

    // Save atlas images
    for atlas in &atlases {
        let path = args
            .output
            .join(format!("{}_{}.png", args.name, atlas.index));
        save_atlas_image(atlas, &path, args.opaque, args.compress)?;
        info!("Saved {}", path.display());
    }

    // Write format-specific output
    match &cli.command {
        Command::Json(_) => {
            write_json(&atlases, &args.output, &args.name)?;
            info!("Generated {}.json", args.name);
        }
        Command::Godot(_) => {
            write_godot_resources(&atlases, &args.output, &args.name, None)?;
            info!(
                "Generated {} Godot .tres files",
                atlases.iter().map(|a| a.sprites.len()).sum::<usize>()
            );
        }
        #[cfg(feature = "gui")]
        Command::Gui => unreachable!(),
    }

    info!("Done!");

    Ok(())
}
