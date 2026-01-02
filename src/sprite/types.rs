use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Trimming information to reconstruct original sprite positioning
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TrimInfo {
    /// Pixels trimmed from left edge
    pub offset_x: i32,
    /// Pixels trimmed from top edge
    pub offset_y: i32,
    /// Original width before trimming
    pub source_width: u32,
    /// Original height before trimming
    pub source_height: u32,
    /// Trimmed width
    pub trimmed_width: u32,
    /// Trimmed height
    pub trimmed_height: u32,
}

impl TrimInfo {
    /// Create TrimInfo for an untrimmed sprite
    pub fn untrimmed(width: u32, height: u32) -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            source_width: width,
            source_height: height,
            trimmed_width: width,
            trimmed_height: height,
        }
    }

    /// Returns true if the sprite was actually trimmed
    pub fn was_trimmed(&self) -> bool {
        self.trimmed_width != self.source_width || self.trimmed_height != self.source_height
    }

    /// Calculate Godot margin values (left, top, right, bottom)
    pub fn godot_margin(&self) -> (i32, i32, i32, i32) {
        let right = self.source_width as i32 - self.trimmed_width as i32 - self.offset_x;
        let bottom = self.source_height as i32 - self.trimmed_height as i32 - self.offset_y;
        (self.offset_x, self.offset_y, right, bottom)
    }
}

/// Represents a source sprite before packing
#[derive(Debug, Clone)]
pub struct SourceSprite {
    /// Original file path
    pub path: PathBuf,
    /// Unique identifier (typically filename without extension)
    pub name: String,
    /// Trimmed image data (transparent borders removed if trimming enabled)
    pub image: RgbaImage,
    /// Trim metadata for offset reconstruction
    pub trim_info: TrimInfo,
}

impl SourceSprite {
    /// Width of the sprite (after trimming)
    pub fn width(&self) -> u32 {
        self.image.width()
    }

    /// Height of the sprite (after trimming)
    pub fn height(&self) -> u32 {
        self.image.height()
    }
}

/// Result of placing a sprite in the atlas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedSprite {
    /// Reference to source sprite name
    pub name: String,
    /// Position in atlas (x coordinate)
    pub x: u32,
    /// Position in atlas (y coordinate)
    pub y: u32,
    /// Width in atlas
    pub width: u32,
    /// Height in atlas
    pub height: u32,
    /// Original trim info for offset calculation
    pub trim_info: TrimInfo,
    /// Index of atlas this sprite belongs to
    pub atlas_index: usize,
}
