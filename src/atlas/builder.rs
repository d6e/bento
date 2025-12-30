use anyhow::Result;
use image::imageops;
use log::info;

use super::Atlas;
use crate::cli::PackingHeuristic;
use crate::error::BentoError;
use crate::packing::MaxRectsPacker;
use crate::sprite::{PackedSprite, SourceSprite};

/// Configuration for atlas building
pub struct AtlasBuilder {
    pub max_width: u32,
    pub max_height: u32,
    pub padding: u32,
    pub heuristic: PackingHeuristic,
    pub power_of_two: bool,
    pub extrude: u32,
}

impl AtlasBuilder {
    pub fn new(max_width: u32, max_height: u32) -> Self {
        Self {
            max_width,
            max_height,
            padding: 1,
            heuristic: PackingHeuristic::BestShortSideFit,
            power_of_two: false,
            extrude: 0,
        }
    }

    pub fn padding(mut self, padding: u32) -> Self {
        self.padding = padding;
        self
    }

    pub fn heuristic(mut self, heuristic: PackingHeuristic) -> Self {
        self.heuristic = heuristic;
        self
    }

    pub fn power_of_two(mut self, pot: bool) -> Self {
        self.power_of_two = pot;
        self
    }

    pub fn extrude(mut self, extrude: u32) -> Self {
        self.extrude = extrude;
        self
    }

    /// Build atlases from the given sprites
    pub fn build(&self, sprites: Vec<SourceSprite>) -> Result<Vec<Atlas>> {
        if sprites.is_empty() {
            return Err(BentoError::NoImages.into());
        }

        // Validate all sprites can fit
        for sprite in &sprites {
            let padded_w = sprite.width() + self.padding * 2 + self.extrude * 2;
            let padded_h = sprite.height() + self.padding * 2 + self.extrude * 2;

            if padded_w > self.max_width || padded_h > self.max_height {
                return Err(BentoError::SpriteTooLarge {
                    name: sprite.name.clone(),
                    width: sprite.width(),
                    height: sprite.height(),
                    max_width: self.max_width,
                    max_height: self.max_height,
                }
                .into());
            }
        }

        let mut atlases = Vec::new();
        let mut remaining: Vec<_> = sprites.into_iter().collect();

        while !remaining.is_empty() {
            let atlas_index = atlases.len();
            let (atlas, unpacked) = self.pack_atlas(atlas_index, remaining)?;
            atlases.push(atlas);
            remaining = unpacked;
        }

        info!(
            "Created {} atlas(es) with {} total sprites",
            atlases.len(),
            atlases.iter().map(|a| a.sprites.len()).sum::<usize>()
        );

        Ok(atlases)
    }

    fn pack_atlas(
        &self,
        index: usize,
        sprites: Vec<SourceSprite>,
    ) -> Result<(Atlas, Vec<SourceSprite>)> {
        let mut packer = MaxRectsPacker::new(self.max_width, self.max_height);
        let mut packed_sprites = Vec::new();
        let mut unpacked = Vec::new();
        let mut max_x = 0u32;
        let mut max_y = 0u32;

        for sprite in sprites {
            let padded_w = sprite.width() + self.padding * 2 + self.extrude * 2;
            let padded_h = sprite.height() + self.padding * 2 + self.extrude * 2;

            if let Some(rect) = packer.insert(padded_w, padded_h, self.heuristic) {
                let sprite_x = rect.x + self.padding + self.extrude;
                let sprite_y = rect.y + self.padding + self.extrude;

                max_x = max_x.max(rect.x + padded_w);
                max_y = max_y.max(rect.y + padded_h);

                packed_sprites.push((
                    PackedSprite {
                        name: sprite.name.clone(),
                        x: sprite_x,
                        y: sprite_y,
                        width: sprite.width(),
                        height: sprite.height(),
                        trim_info: sprite.trim_info,
                        atlas_index: index,
                    },
                    sprite,
                ));
            } else {
                unpacked.push(sprite);
            }
        }

        // Determine final atlas dimensions
        let (final_width, final_height) = if self.power_of_two {
            (next_power_of_two(max_x), next_power_of_two(max_y))
        } else {
            (max_x, max_y)
        };

        let mut atlas = Atlas::new(index, final_width, final_height);

        // Render sprites to atlas
        for (packed, source) in packed_sprites {
            // Handle extrusion
            if self.extrude > 0 {
                self.extrude_sprite(&mut atlas.image, &source, packed.x, packed.y);
            }

            // Copy sprite image
            imageops::overlay(&mut atlas.image, &source.image, packed.x as i64, packed.y as i64);

            atlas.sprites.push(packed);
        }

        info!(
            "Atlas {}: {}x{} with {} sprites ({:.1}% efficiency)",
            index,
            final_width,
            final_height,
            atlas.sprites.len(),
            packer.occupancy() * 100.0
        );

        Ok((atlas, unpacked))
    }

    fn extrude_sprite(
        &self,
        atlas: &mut image::RgbaImage,
        sprite: &SourceSprite,
        x: u32,
        y: u32,
    ) {
        let img = &sprite.image;
        let (w, h) = img.dimensions();

        // Extrude edges
        for e in 1..=self.extrude {
            // Top edge
            if y >= e {
                for sx in 0..w {
                    let pixel = img.get_pixel(sx, 0);
                    atlas.put_pixel(x + sx, y - e, *pixel);
                }
            }

            // Bottom edge
            for sx in 0..w {
                let pixel = img.get_pixel(sx, h - 1);
                atlas.put_pixel(x + sx, y + h - 1 + e, *pixel);
            }

            // Left edge
            if x >= e {
                for sy in 0..h {
                    let pixel = img.get_pixel(0, sy);
                    atlas.put_pixel(x - e, y + sy, *pixel);
                }
            }

            // Right edge
            for sy in 0..h {
                let pixel = img.get_pixel(w - 1, sy);
                atlas.put_pixel(x + w - 1 + e, y + sy, *pixel);
            }

            // Corners
            if x >= e && y >= e {
                let pixel = img.get_pixel(0, 0);
                atlas.put_pixel(x - e, y - e, *pixel);
            }
            if y >= e {
                let pixel = img.get_pixel(w - 1, 0);
                atlas.put_pixel(x + w - 1 + e, y - e, *pixel);
            }
            if x >= e {
                let pixel = img.get_pixel(0, h - 1);
                atlas.put_pixel(x - e, y + h - 1 + e, *pixel);
            }
            let pixel = img.get_pixel(w - 1, h - 1);
            atlas.put_pixel(x + w - 1 + e, y + h - 1 + e, *pixel);
        }
    }
}

fn next_power_of_two(n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut v = n - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_power_of_two() {
        assert_eq!(next_power_of_two(0), 1);
        assert_eq!(next_power_of_two(1), 1);
        assert_eq!(next_power_of_two(2), 2);
        assert_eq!(next_power_of_two(3), 4);
        assert_eq!(next_power_of_two(5), 8);
        assert_eq!(next_power_of_two(100), 128);
        assert_eq!(next_power_of_two(1000), 1024);
    }
}
