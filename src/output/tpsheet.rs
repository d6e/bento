use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::atlas::Atlas;
use crate::error::BentoError;
use crate::sprite::PackedSprite;

#[derive(Serialize)]
struct TpsheetOutput {
    textures: Vec<TpTexture>,
    meta: TpMeta,
}

#[derive(Serialize)]
struct TpTexture {
    image: String,
    size: TpSize,
    sprites: Vec<TpSprite>,
}

#[derive(Serialize)]
struct TpSize {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct TpSprite {
    filename: String,
    region: TpRegion,
    margin: TpMargin,
}

#[derive(Serialize)]
struct TpRegion {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct TpMargin {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct TpMeta {
    app: &'static str,
    version: &'static str,
}

/// Write TexturePacker .tpsheet metadata file
pub fn write_tpsheet(atlases: &[Atlas], output_dir: &Path, base_name: &str) -> Result<()> {
    let textures: Vec<_> = atlases
        .iter()
        .map(|atlas| {
            let image = format!("{}_{}.png", base_name, atlas.index);
            let sprites = atlas.sprites.iter().map(sprite_to_tpsprite).collect();

            TpTexture {
                image,
                size: TpSize {
                    w: atlas.width,
                    h: atlas.height,
                },
                sprites,
            }
        })
        .collect();

    let output = TpsheetOutput {
        textures,
        meta: TpMeta {
            app: "bento",
            version: "1.0",
        },
    };

    let tpsheet_path = output_dir.join(format!("{}.tpsheet", base_name));
    let content = serde_json::to_string_pretty(&output)?;

    fs::write(&tpsheet_path, content).map_err(|e| BentoError::OutputWrite {
        path: tpsheet_path,
        source: e,
    })?;

    Ok(())
}

fn sprite_to_tpsprite(sprite: &PackedSprite) -> TpSprite {
    let trim = &sprite.trim_info;

    TpSprite {
        filename: sprite.name.clone(),
        region: TpRegion {
            x: sprite.x,
            y: sprite.y,
            w: sprite.width,
            h: sprite.height,
        },
        margin: TpMargin {
            x: trim.offset_x,
            y: trim.offset_y,
            w: trim.source_width - trim.trimmed_width,
            h: trim.source_height - trim.trimmed_height,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::TrimInfo;

    #[test]
    fn test_sprite_to_tpsprite_untrimmed() {
        let sprite = PackedSprite {
            name: "sprite1.png".to_string(),
            x: 10,
            y: 20,
            width: 32,
            height: 32,
            trim_info: TrimInfo::untrimmed(32, 32),
            atlas_index: 0,
        };

        let tp = sprite_to_tpsprite(&sprite);

        assert_eq!(tp.filename, "sprite1.png");
        assert_eq!(tp.region.x, 10);
        assert_eq!(tp.region.y, 20);
        assert_eq!(tp.region.w, 32);
        assert_eq!(tp.region.h, 32);
        assert_eq!(tp.margin.x, 0);
        assert_eq!(tp.margin.y, 0);
        assert_eq!(tp.margin.w, 0);
        assert_eq!(tp.margin.h, 0);
    }

    #[test]
    fn test_sprite_to_tpsprite_trimmed() {
        let sprite = PackedSprite {
            name: "folder/sprite2.png".to_string(),
            x: 34,
            y: 0,
            width: 28,
            height: 30,
            trim_info: TrimInfo {
                offset_x: 2,
                offset_y: 1,
                source_width: 32,
                source_height: 32,
                trimmed_width: 28,
                trimmed_height: 30,
            },
            atlas_index: 0,
        };

        let tp = sprite_to_tpsprite(&sprite);

        assert_eq!(tp.filename, "folder/sprite2.png");
        assert_eq!(tp.region.x, 34);
        assert_eq!(tp.region.y, 0);
        assert_eq!(tp.region.w, 28);
        assert_eq!(tp.region.h, 30);
        assert_eq!(tp.margin.x, 2);
        assert_eq!(tp.margin.y, 1);
        assert_eq!(tp.margin.w, 4); // 32 - 28
        assert_eq!(tp.margin.h, 2); // 32 - 30
    }
}
