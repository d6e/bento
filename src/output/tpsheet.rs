use std::fs;
use std::path::Path;

use anyhow::Result;
use image::RgbaImage;
use serde::Serialize;

use crate::atlas::Atlas;
use crate::error::BentoError;
use crate::output::atlas_png_filename;
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
    /// See [`pixel_hash`]. Lets a consumer check that the tpsheet still describes this png.
    pixel_hash: String,
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

/// Write TexturePacker .tpsheet metadata file. `opaque` must match how the png was saved,
/// since it changes the pixels the png holds.
pub fn write_tpsheet(
    atlases: &[Atlas],
    output_dir: &Path,
    base_name: &str,
    opaque: bool,
) -> Result<()> {
    let total = atlases.len();
    let textures: Vec<_> = atlases
        .iter()
        .map(|atlas| {
            let image = atlas_png_filename(base_name, atlas.index, total);
            let sprites = atlas
                .sprites
                .iter()
                .map(|sprite| sprite_to_tpsprite(sprite, &atlas.image, opaque))
                .collect();

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

/// FNV-1a 64 over the sprite's region as the saved png decodes, as 16 lowercase hex digits.
///
/// Pixels are RGBA8, row by row. Two normalisations make it match what a decoder reads back:
/// an opaque atlas is saved as RGB, so alpha counts as 255, and a fully transparent pixel counts
/// as all zeroes, since png optimisers are free to change the colour of invisible pixels.
/// A consumer hashes the decoded png with `opaque: false`; an opaque png decodes with alpha 255.
pub fn pixel_hash(image: &RgbaImage, x: u32, y: u32, w: u32, h: u32, opaque: bool) -> String {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for py in y..y + h {
        for px in x..x + w {
            let [r, g, b, a] = image.get_pixel(px, py).0;
            let pixel = if opaque {
                [r, g, b, 255]
            } else if a == 0 {
                [0, 0, 0, 0]
            } else {
                [r, g, b, a]
            };
            for byte in pixel {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(PRIME);
            }
        }
    }
    format!("{:016x}", hash)
}

fn sprite_to_tpsprite(sprite: &PackedSprite, image: &RgbaImage, opaque: bool) -> TpSprite {
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
        pixel_hash: pixel_hash(
            image,
            sprite.x,
            sprite.y,
            sprite.width,
            sprite.height,
            opaque,
        ),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::cli::CompressionLevel;
    use crate::output::save_atlas_image;
    use crate::sprite::TrimInfo;
    use image::Rgba;

    fn gradient(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            Rgba([(x * 7) as u8, (y * 13) as u8, (x * y) as u8, 255])
        })
    }

    #[test]
    fn test_pixel_hash_changes_when_one_pixel_changes() {
        let mut image = gradient(8, 8);
        let before = pixel_hash(&image, 2, 2, 4, 4, false);
        image.put_pixel(5, 5, Rgba([0, 0, 0, 255]));

        assert_ne!(pixel_hash(&image, 2, 2, 4, 4, false), before);
    }

    #[test]
    fn test_pixel_hash_ignores_pixels_outside_the_region() {
        let mut image = gradient(8, 8);
        let before = pixel_hash(&image, 2, 2, 4, 4, false);
        image.put_pixel(6, 6, Rgba([0, 0, 0, 255]));

        assert_eq!(pixel_hash(&image, 2, 2, 4, 4, false), before);
    }

    #[test]
    fn test_pixel_hash_ignores_colour_of_transparent_pixels() {
        let mut image = gradient(4, 4);
        image.put_pixel(1, 1, Rgba([10, 20, 30, 0]));
        let before = pixel_hash(&image, 0, 0, 4, 4, false);
        image.put_pixel(1, 1, Rgba([200, 100, 50, 0]));

        assert_eq!(pixel_hash(&image, 0, 0, 4, 4, false), before);
    }

    #[test]
    fn test_pixel_hash_ignores_alpha_when_opaque() {
        let mut image = gradient(4, 4);
        let before = pixel_hash(&image, 0, 0, 4, 4, true);
        image.put_pixel(1, 1, Rgba([7, 13, 1, 0]));

        assert_eq!(pixel_hash(&image, 0, 0, 4, 4, true), before);
    }

    /// The hash is only useful if a consumer reading the saved png gets the same value, so
    /// round-trip through the real save path with recompression on, for both colour modes.
    #[test]
    fn test_pixel_hash_survives_saving_and_decoding_the_png() {
        for opaque in [false, true] {
            let mut atlas = Atlas::new(0, 16, 16);
            atlas.image = RgbaImage::from_fn(16, 16, |x, y| {
                let alpha = if (x + y) % 3 == 0 {
                    0
                } else {
                    255 - (x * 9) as u8
                };
                Rgba([(x * 17) as u8, (y * 5) as u8, 99, alpha])
            });
            atlas.sprites.push(PackedSprite {
                name: "a.png".to_string(),
                x: 1,
                y: 2,
                width: 9,
                height: 11,
                trim_info: TrimInfo::untrimmed(9, 11),
                atlas_index: 0,
            });

            let dir = std::env::temp_dir().join(format!("bento_test_pixel_hash_{}", opaque));
            std::fs::create_dir_all(&dir).unwrap();
            let png = dir.join("atlas.png");
            save_atlas_image(&atlas, &png, opaque, Some(CompressionLevel::Level(2))).unwrap();
            let decoded = image::open(&png).unwrap().to_rgba8();
            std::fs::remove_dir_all(&dir).unwrap();

            let written = sprite_to_tpsprite(&atlas.sprites[0], &atlas.image, opaque).pixel_hash;
            // A consumer does not know the colour mode; it hashes what it decodes.
            assert_eq!(
                pixel_hash(&decoded, 1, 2, 9, 11, false),
                written,
                "opaque={}",
                opaque
            );
        }
    }

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

        let tp = sprite_to_tpsprite(&sprite, &RgbaImage::new(64, 64), false);

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

        let tp = sprite_to_tpsprite(&sprite, &RgbaImage::new(64, 64), false);

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
