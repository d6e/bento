use std::path::Path;

use anyhow::Result;
use image::{DynamicImage, ImageFormat, RgbImage};

use crate::atlas::Atlas;
use crate::cli::OutputFormat;
use crate::error::BentoError;

/// Save atlas image as PNG
pub fn save_atlas_image(atlas: &Atlas, path: &Path, opaque: bool) -> Result<()> {
    if opaque {
        let rgb: RgbImage = DynamicImage::ImageRgba8(atlas.image.clone()).into_rgb8();
        rgb.save_with_format(path, ImageFormat::Png)
            .map_err(|e| BentoError::ImageSave {
                path: path.to_path_buf(),
                source: e,
            })?;
    } else {
        atlas
            .image
            .save_with_format(path, ImageFormat::Png)
            .map_err(|e| BentoError::ImageSave {
                path: path.to_path_buf(),
                source: e,
            })?;
    }

    Ok(())
}

impl OutputFormat {
    pub fn should_write_godot(&self) -> bool {
        matches!(self, OutputFormat::Godot | OutputFormat::Both)
    }

    pub fn should_write_json(&self) -> bool {
        matches!(self, OutputFormat::Json | OutputFormat::Both)
    }
}
