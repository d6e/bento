use image::RgbaImage;

use crate::sprite::PackedSprite;

/// A completed texture atlas
#[derive(Debug)]
pub struct Atlas {
    /// Atlas index (for multi-atlas support)
    pub index: usize,
    /// Atlas width
    pub width: u32,
    /// Atlas height
    pub height: u32,
    /// Rendered atlas image
    pub image: RgbaImage,
    /// All sprites packed into this atlas
    pub sprites: Vec<PackedSprite>,
}

impl Atlas {
    pub fn new(index: usize, width: u32, height: u32) -> Self {
        Self {
            index,
            width,
            height,
            image: RgbaImage::new(width, height),
            sprites: Vec::new(),
        }
    }
}
