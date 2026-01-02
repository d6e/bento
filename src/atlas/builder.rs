use anyhow::Result;
use image::imageops;
use log::{debug, info};

use super::Atlas;
use crate::cli::{PackMode, PackingHeuristic};
use crate::error::BentoError;
use crate::packing::MaxRectsPacker;
use crate::sprite::{PackedSprite, SourceSprite};

/// All concrete heuristics to try when using "Best" mode
const ALL_HEURISTICS: [PackingHeuristic; 5] = [
    PackingHeuristic::BestShortSideFit,
    PackingHeuristic::BestLongSideFit,
    PackingHeuristic::BestAreaFit,
    PackingHeuristic::BottomLeft,
    PackingHeuristic::ContactPoint,
];

/// Sprite ordering strategies for pack-mode best
#[derive(Debug, Clone, Copy)]
enum SpriteOrdering {
    /// Keep original input order
    Original,
    /// Sort by area (largest first)
    ByArea,
    /// Sort by perimeter (largest first)
    ByPerimeter,
    /// Sort by max dimension (largest first)
    ByMaxDimension,
    /// Sort by width (widest first)
    ByWidth,
    /// Sort by height (tallest first)
    ByHeight,
    /// Sort by aspect ratio extremity (furthest from 1:1 first)
    ByWidthHeightRatio,
    /// Sort by diagonal length (largest first)
    ByDiagonal,
}

const ALL_ORDERINGS: [SpriteOrdering; 8] = [
    SpriteOrdering::Original,
    SpriteOrdering::ByArea,
    SpriteOrdering::ByPerimeter,
    SpriteOrdering::ByMaxDimension,
    SpriteOrdering::ByWidth,
    SpriteOrdering::ByHeight,
    SpriteOrdering::ByWidthHeightRatio,
    SpriteOrdering::ByDiagonal,
];

/// Configuration for atlas building
pub struct AtlasBuilder {
    pub max_width: u32,
    pub max_height: u32,
    pub padding: u32,
    pub heuristic: PackingHeuristic,
    pub power_of_two: bool,
    pub extrude: u32,
    pub pack_mode: PackMode,
}

/// Intermediate placement info for a single sprite
struct SpritePlacement {
    sprite_index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    name: String,
    trim_info: crate::sprite::TrimInfo,
    atlas_index: usize,
}

/// Result of trying a packing heuristic
struct PackingLayout {
    placements: Vec<SpritePlacement>,
    unpacked_indices: Vec<usize>,
    max_x: u32,
    max_y: u32,
    occupancy: f64,
}

impl PackingLayout {
    /// Returns true if this layout is better than another.
    /// Priority: 1) more sprites packed, 2) smaller atlas area, 3) higher occupancy.
    fn is_better_than(&self, other: &PackingLayout) -> bool {
        let self_packed = self.placements.len();
        let other_packed = other.placements.len();

        if self_packed != other_packed {
            return self_packed > other_packed;
        }

        // Same sprite count - prefer smaller atlas area
        let self_area = u64::from(self.max_x) * u64::from(self.max_y);
        let other_area = u64::from(other.max_x) * u64::from(other.max_y);

        if self_area != other_area {
            return self_area < other_area;
        }

        // Same area - prefer higher occupancy (denser packing)
        self.occupancy > other.occupancy
    }
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
            pack_mode: PackMode::Single,
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

    pub fn pack_mode(mut self, pack_mode: PackMode) -> Self {
        self.pack_mode = pack_mode;
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
        // If Best heuristic mode, try all heuristics (and orderings if pack_mode is Best)
        let (best_heuristic, best_ordering, best_layout) =
            if self.heuristic == PackingHeuristic::Best {
                self.find_best_heuristic(&sprites, index)
            } else {
                // Use specified heuristic with original ordering (or try orderings if pack_mode is Best)
                let orderings: &[SpriteOrdering] = if self.pack_mode == PackMode::Best {
                    &ALL_ORDERINGS
                } else {
                    &[SpriteOrdering::Original]
                };

                let mut best: Option<(SpriteOrdering, PackingLayout)> = None;
                for &ordering in orderings {
                    let order = self.sorted_indices(&sprites, ordering);
                    let layout = self.try_pack(&sprites, &order, index, self.heuristic);

                    let dominated = best
                        .as_ref()
                        .is_some_and(|(_, b)| !layout.is_better_than(b));
                    if !dominated {
                        best = Some((ordering, layout));
                    }
                }

                // Orderings slice is non-empty (contains at least Original)
                #[expect(clippy::expect_used, reason = "orderings is non-empty")]
                let (ordering, layout) = best.expect("at least one ordering should be tried");
                (self.heuristic, ordering, layout)
            };

        // Apply the best layout
        self.apply_layout(index, sprites, best_heuristic, best_ordering, best_layout)
    }

    /// Try packing with a specific heuristic and ordering, return placement info without rendering
    fn try_pack(
        &self,
        sprites: &[SourceSprite],
        order: &[usize],
        index: usize,
        heuristic: PackingHeuristic,
    ) -> PackingLayout {
        let mut packer = MaxRectsPacker::new(self.max_width, self.max_height);
        let mut placements = Vec::new();
        let mut unpacked_indices = Vec::new();
        let mut max_x = 0u32;
        let mut max_y = 0u32;

        for &i in order {
            let sprite = &sprites[i];
            let padded_w = sprite.width() + self.padding * 2 + self.extrude * 2;
            let padded_h = sprite.height() + self.padding * 2 + self.extrude * 2;

            if let Some(rect) = packer.insert(padded_w, padded_h, heuristic) {
                let sprite_x = rect.x + self.padding + self.extrude;
                let sprite_y = rect.y + self.padding + self.extrude;

                max_x = max_x.max(rect.x + padded_w);
                max_y = max_y.max(rect.y + padded_h);

                placements.push(SpritePlacement {
                    sprite_index: i,
                    x: sprite_x,
                    y: sprite_y,
                    width: sprite.width(),
                    height: sprite.height(),
                    name: sprite.name.clone(),
                    trim_info: sprite.trim_info,
                    atlas_index: index,
                });
            } else {
                unpacked_indices.push(i);
            }
        }

        PackingLayout {
            placements,
            unpacked_indices,
            max_x,
            max_y,
            occupancy: packer.occupancy(),
        }
    }

    /// Create sorted indices for a given ordering strategy
    fn sorted_indices(&self, sprites: &[SourceSprite], ordering: SpriteOrdering) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..sprites.len()).collect();

        match ordering {
            SpriteOrdering::Original => {}
            SpriteOrdering::ByArea => {
                indices.sort_by(|&a, &b| {
                    let area_a = u64::from(sprites[a].width()) * u64::from(sprites[a].height());
                    let area_b = u64::from(sprites[b].width()) * u64::from(sprites[b].height());
                    area_b.cmp(&area_a) // descending
                });
            }
            SpriteOrdering::ByPerimeter => {
                indices.sort_by(|&a, &b| {
                    let perim_a = u64::from(sprites[a].width()) + u64::from(sprites[a].height());
                    let perim_b = u64::from(sprites[b].width()) + u64::from(sprites[b].height());
                    perim_b.cmp(&perim_a) // descending
                });
            }
            SpriteOrdering::ByMaxDimension => {
                indices.sort_by(|&a, &b| {
                    let max_a = sprites[a].width().max(sprites[a].height());
                    let max_b = sprites[b].width().max(sprites[b].height());
                    max_b.cmp(&max_a) // descending
                });
            }
            SpriteOrdering::ByWidth => {
                indices.sort_by(|&a, &b| {
                    sprites[b].width().cmp(&sprites[a].width()) // descending
                });
            }
            SpriteOrdering::ByHeight => {
                indices.sort_by(|&a, &b| {
                    sprites[b].height().cmp(&sprites[a].height()) // descending
                });
            }
            SpriteOrdering::ByWidthHeightRatio => {
                // Sort by how far the aspect ratio is from 1:1 (most extreme first)
                indices.sort_by(|&a, &b| {
                    let w_a = f64::from(sprites[a].width().max(1));
                    let h_a = f64::from(sprites[a].height().max(1));
                    let w_b = f64::from(sprites[b].width().max(1));
                    let h_b = f64::from(sprites[b].height().max(1));
                    // Ratio is max/min, so always >= 1.0. Higher = more extreme.
                    let ratio_a = w_a.max(h_a) / w_a.min(h_a);
                    let ratio_b = w_b.max(h_b) / w_b.min(h_b);
                    ratio_b
                        .partial_cmp(&ratio_a)
                        .unwrap_or(std::cmp::Ordering::Equal) // descending
                });
            }
            SpriteOrdering::ByDiagonal => {
                // Sort by diagonal length (sqrt(w^2 + h^2)), largest first
                indices.sort_by(|&a, &b| {
                    let diag_sq_a =
                        u64::from(sprites[a].width()).pow(2) + u64::from(sprites[a].height()).pow(2);
                    let diag_sq_b =
                        u64::from(sprites[b].width()).pow(2) + u64::from(sprites[b].height()).pow(2);
                    diag_sq_b.cmp(&diag_sq_a) // descending (compare squared to avoid sqrt)
                });
            }
        }

        indices
    }

    /// Find the best heuristic (and ordering if pack_mode is Best)
    fn find_best_heuristic(
        &self,
        sprites: &[SourceSprite],
        index: usize,
    ) -> (PackingHeuristic, SpriteOrdering, PackingLayout) {
        let mut best: Option<(PackingHeuristic, SpriteOrdering, PackingLayout)> = None;

        // Determine which orderings to try
        let orderings: &[SpriteOrdering] = if self.pack_mode == PackMode::Best {
            &ALL_ORDERINGS
        } else {
            &[SpriteOrdering::Original]
        };

        for &ordering in orderings {
            let order = self.sorted_indices(sprites, ordering);

            for &heuristic in &ALL_HEURISTICS {
                let layout = self.try_pack(sprites, &order, index, heuristic);

                let dominated = best
                    .as_ref()
                    .is_some_and(|(_, _, b)| !layout.is_better_than(b));

                if !dominated {
                    debug!(
                        "Ordering {:?} + Heuristic {:?}: packed {}/{}, occupancy {:.1}%",
                        ordering,
                        heuristic,
                        layout.placements.len(),
                        sprites.len(),
                        layout.occupancy * 100.0
                    );
                    best = Some((heuristic, ordering, layout));
                }
            }
        }

        // ALL_HEURISTICS and orderings are non-empty
        #[expect(clippy::expect_used, reason = "heuristics and orderings are non-empty")]
        best.expect("at least one heuristic should be tried")
    }

    /// Apply a computed layout to produce the final atlas
    fn apply_layout(
        &self,
        index: usize,
        sprites: Vec<SourceSprite>,
        heuristic: PackingHeuristic,
        ordering: SpriteOrdering,
        layout: PackingLayout,
    ) -> Result<(Atlas, Vec<SourceSprite>)> {
        let (final_width, final_height) = if self.power_of_two {
            (
                next_power_of_two(layout.max_x),
                next_power_of_two(layout.max_y),
            )
        } else {
            (layout.max_x, layout.max_y)
        };

        let mut atlas = Atlas::new(index, final_width, final_height);
        atlas.occupancy = layout.occupancy;

        // Convert sprites vec to allow indexed access
        let mut sprites: Vec<Option<SourceSprite>> = sprites.into_iter().map(Some).collect();
        let mut unpacked = Vec::new();

        // Render packed sprites
        for placement in layout.placements {
            // Each sprite_index appears exactly once in placements
            #[expect(clippy::expect_used, reason = "sprite indices are unique")]
            let source = sprites[placement.sprite_index]
                .take()
                .expect("sprite should exist");

            if self.extrude > 0 {
                self.extrude_sprite(&mut atlas.image, &source, placement.x, placement.y);
            }

            imageops::overlay(
                &mut atlas.image,
                &source.image,
                i64::from(placement.x),
                i64::from(placement.y),
            );

            atlas.sprites.push(PackedSprite {
                name: placement.name,
                x: placement.x,
                y: placement.y,
                width: placement.width,
                height: placement.height,
                trim_info: placement.trim_info,
                atlas_index: placement.atlas_index,
            });
        }

        // Collect unpacked sprites
        for idx in layout.unpacked_indices {
            if let Some(sprite) = sprites[idx].take() {
                unpacked.push(sprite);
            }
        }

        let optimization_info = match (
            self.heuristic == PackingHeuristic::Best,
            self.pack_mode == PackMode::Best,
        ) {
            (true, true) => format!(" (best: {:?}, {:?})", heuristic, ordering),
            (true, false) => format!(" (best: {:?})", heuristic),
            (false, true) => format!(" (ordering: {:?})", ordering),
            (false, false) => String::new(),
        };

        info!(
            "Atlas {}: {}x{} with {} sprites ({:.1}% efficiency){}",
            index,
            final_width,
            final_height,
            atlas.sprites.len(),
            layout.occupancy * 100.0,
            optimization_info,
        );

        Ok((atlas, unpacked))
    }

    fn extrude_sprite(&self, atlas: &mut image::RgbaImage, sprite: &SourceSprite, x: u32, y: u32) {
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
    use crate::sprite::TrimInfo;
    use image::Rgba;

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

    #[test]
    fn test_extrusion_with_padding_prevents_underflow() {
        // Test that extrusion doesn't cause underflow when sprite is placed at origin.
        // The padding + extrude offset ensures sprite_x/y are always >= extrude.
        //
        // With padding=1, extrude=2:
        // - padded size = sprite + 2*padding + 2*extrude = sprite + 2 + 4 = sprite + 6
        // - MaxRects places at rect.x=0, rect.y=0
        // - sprite_x = 0 + 1 + 2 = 3
        // - sprite_y = 0 + 1 + 2 = 3
        // - When extruding, we need x >= extrude (3 >= 2) and y >= extrude (3 >= 2)
        // - This is always true because sprite_x = rect.x + padding + extrude >= extrude

        let mut sprite_img = image::RgbaImage::new(4, 4);
        for pixel in sprite_img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let sprites = vec![SourceSprite {
            path: std::path::PathBuf::from("test.png"),
            name: "test".to_string(),
            image: sprite_img,
            trim_info: TrimInfo::untrimmed(4, 4),
        }];

        let builder = AtlasBuilder::new(256, 256).padding(1).extrude(2);

        // This should not panic
        let result = builder.build(sprites);
        assert!(result.is_ok());

        let atlases = result.unwrap();
        assert_eq!(atlases.len(), 1);
        assert_eq!(atlases[0].sprites.len(), 1);

        // Verify sprite position accounts for padding + extrude
        let packed = &atlases[0].sprites[0];
        assert_eq!(packed.x, 3); // 0 + 1 (padding) + 2 (extrude)
        assert_eq!(packed.y, 3);
    }

    #[test]
    fn test_extrusion_zero_padding() {
        // With padding=0, extrude=1:
        // - sprite_x = 0 + 0 + 1 = 1
        // - Extrusion needs x >= 1, which is satisfied (1 >= 1)

        let mut sprite_img = image::RgbaImage::new(4, 4);
        for pixel in sprite_img.pixels_mut() {
            *pixel = Rgba([0, 255, 0, 255]);
        }

        let sprites = vec![SourceSprite {
            path: std::path::PathBuf::from("test.png"),
            name: "test".to_string(),
            image: sprite_img,
            trim_info: TrimInfo::untrimmed(4, 4),
        }];

        let builder = AtlasBuilder::new(256, 256).padding(0).extrude(1);

        let result = builder.build(sprites);
        assert!(result.is_ok());

        let packed = &result.unwrap()[0].sprites[0];
        assert_eq!(packed.x, 1); // 0 + 0 + 1
        assert_eq!(packed.y, 1);
    }

    #[test]
    fn test_best_heuristic_packs_all_sprites() {
        // Best mode should try all heuristics and pick the best result.
        // Create sprites that should all fit in one atlas.
        let mut sprites = Vec::new();
        for i in 0..4 {
            let mut img = image::RgbaImage::new(20, 20);
            for pixel in img.pixels_mut() {
                *pixel = Rgba([255, 0, 0, 255]);
            }
            sprites.push(SourceSprite {
                path: std::path::PathBuf::from(format!("sprite_{}.png", i)),
                name: format!("sprite_{}", i),
                image: img,
                trim_info: TrimInfo::untrimmed(20, 20),
            });
        }

        let builder = AtlasBuilder::new(100, 100)
            .padding(1)
            .heuristic(PackingHeuristic::Best);

        let result = builder.build(sprites);
        assert!(result.is_ok());

        let atlases = result.unwrap();
        assert_eq!(atlases.len(), 1, "All sprites should fit in one atlas");
        assert_eq!(
            atlases[0].sprites.len(),
            4,
            "All 4 sprites should be packed"
        );
    }

    #[test]
    fn test_best_heuristic_produces_valid_result() {
        // Best mode should produce a result at least as good as any single heuristic.
        let create_sprites = || {
            let mut sprites = Vec::new();
            let sizes = [(30, 20), (25, 15), (40, 10), (15, 35), (20, 20)];
            for (i, (w, h)) in sizes.iter().enumerate() {
                let img = image::RgbaImage::new(*w, *h);
                sprites.push(SourceSprite {
                    path: std::path::PathBuf::from(format!("sprite_{}.png", i)),
                    name: format!("sprite_{}", i),
                    image: img,
                    trim_info: TrimInfo::untrimmed(*w, *h),
                });
            }
            sprites
        };

        // Pack with Best mode
        let best_builder = AtlasBuilder::new(100, 100)
            .padding(0)
            .heuristic(PackingHeuristic::Best);
        let best_result = best_builder.build(create_sprites()).unwrap();
        let best_packed = best_result[0].sprites.len();

        // Best should pack at least as many as any single heuristic
        for heuristic in ALL_HEURISTICS {
            let builder = AtlasBuilder::new(100, 100).padding(0).heuristic(heuristic);
            let result = builder.build(create_sprites()).unwrap();
            let packed = result[0].sprites.len();

            assert!(
                best_packed >= packed,
                "Best mode ({} packed) should be >= {:?} ({} packed)",
                best_packed,
                heuristic,
                packed
            );
        }
    }

    #[test]
    fn test_pack_mode_best_with_orderings() {
        // Test that pack_mode Best actually improves results over Single for pathological cases.
        //
        // Bin: 100x60
        // Sprites in input order: 80x30, 40x50, 50x50
        //
        // Original order packs only 1 sprite:
        //   - 80x30 at (0,0) leaves 20x60 right + 100x30 bottom
        //   - 40x50 needs 50 height, neither region has it
        //   - 50x50 same problem
        //
        // ByArea order (50x50, 80x30, 40x50) packs 2 sprites:
        //   - 50x50 at (0,0) leaves 50x60 right + 100x10 bottom
        //   - 80x30 doesn't fit (needs 80 width or 30 height)
        //   - 40x50 fits in 50x60 right region
        let create_sprites = || {
            let sizes = [(80, 30), (40, 50), (50, 50)];
            sizes
                .iter()
                .enumerate()
                .map(|(i, (w, h))| SourceSprite {
                    path: std::path::PathBuf::from(format!("sprite_{}.png", i)),
                    name: format!("sprite_{}", i),
                    image: image::RgbaImage::new(*w, *h),
                    trim_info: TrimInfo::untrimmed(*w, *h),
                })
                .collect::<Vec<_>>()
        };

        // Pack with pack_mode Single (original ordering only)
        let single_builder = AtlasBuilder::new(100, 60)
            .padding(0)
            .heuristic(PackingHeuristic::BestShortSideFit)
            .pack_mode(PackMode::Single);
        let single_result = single_builder.build(create_sprites()).unwrap();
        let single_packed = single_result[0].sprites.len();

        // Pack with pack_mode Best (tries multiple orderings)
        let best_builder = AtlasBuilder::new(100, 60)
            .padding(0)
            .heuristic(PackingHeuristic::BestShortSideFit)
            .pack_mode(PackMode::Best);
        let best_result = best_builder.build(create_sprites()).unwrap();
        let best_packed = best_result[0].sprites.len();

        // Best mode should pack MORE sprites for this pathological input order
        assert!(
            best_packed > single_packed,
            "pack_mode Best ({}) should pack more than Single ({}) for this case",
            best_packed,
            single_packed
        );
    }
}
