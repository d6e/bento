use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::atlas::Atlas;
use crate::error::BentoError;
use crate::output::atlas_png_filename;
use crate::sprite::PackedSprite;

#[derive(Serialize)]
struct JsonOutput {
    meta: Meta,
    atlases: Vec<JsonAtlas>,
}

#[derive(Serialize)]
struct Meta {
    app: &'static str,
    version: &'static str,
    format: &'static str,
}

#[derive(Serialize)]
struct JsonAtlas {
    image: String,
    size: Size,
    sprites: Vec<JsonSprite>,
}

#[derive(Serialize)]
struct Size {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSprite {
    name: String,
    frame: Frame,
    trimmed: bool,
    sprite_source_size: Frame,
    source_size: Size,
}

#[derive(Serialize)]
struct Frame {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Write JSON metadata file
pub fn write_json(atlases: &[Atlas], output_dir: &Path, base_name: &str) -> Result<()> {
    let total = atlases.len();
    let json_atlases: Vec<_> = atlases
        .iter()
        .map(|atlas| {
            let image = atlas_png_filename(base_name, atlas.index, total);
            let sprites = atlas.sprites.iter().map(sprite_to_json).collect();

            JsonAtlas {
                image,
                size: Size {
                    w: atlas.width,
                    h: atlas.height,
                },
                sprites,
            }
        })
        .collect();

    let output = JsonOutput {
        meta: Meta {
            app: "bento",
            version: env!("CARGO_PKG_VERSION"),
            format: "rgba8888",
        },
        atlases: json_atlases,
    };

    let json_path = output_dir.join(format!("{}.json", base_name));
    let content = serde_json::to_string_pretty(&output)?;

    fs::write(&json_path, content).map_err(|e| BentoError::OutputWrite {
        path: json_path,
        source: e,
    })?;

    Ok(())
}

fn sprite_to_json(sprite: &PackedSprite) -> JsonSprite {
    let trim = &sprite.trim_info;

    JsonSprite {
        name: sprite.name.clone(),
        frame: Frame {
            x: sprite.x,
            y: sprite.y,
            w: sprite.width,
            h: sprite.height,
        },
        trimmed: trim.was_trimmed(),
        // offset_x/offset_y are always >= 0 (pixels trimmed from left/top edge)
        #[expect(
            clippy::cast_sign_loss,
            reason = "trim offsets are always non-negative"
        )]
        sprite_source_size: Frame {
            x: trim.offset_x as u32,
            y: trim.offset_y as u32,
            w: trim.trimmed_width,
            h: trim.trimmed_height,
        },
        source_size: Size {
            w: trim.source_width,
            h: trim.source_height,
        },
    }
}
