use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::Result;
use image::{DynamicImage, ImageFormat, RgbImage};

use crate::atlas::Atlas;
use crate::cli::{CompressionLevel, OutputFormat};
use crate::error::BentoError;

/// Save atlas image as PNG, optionally with compression
pub fn save_atlas_image(
    atlas: &Atlas,
    path: &Path,
    opaque: bool,
    compress: Option<CompressionLevel>,
) -> Result<()> {
    // Encode to PNG in memory
    let mut png_data = Cursor::new(Vec::new());
    if opaque {
        let rgb: RgbImage = DynamicImage::ImageRgba8(atlas.image.clone()).into_rgb8();
        rgb.write_to(&mut png_data, ImageFormat::Png)
            .map_err(|e| BentoError::ImageSave {
                path: path.to_path_buf(),
                source: e,
            })?;
    } else {
        atlas
            .image
            .write_to(&mut png_data, ImageFormat::Png)
            .map_err(|e| BentoError::ImageSave {
                path: path.to_path_buf(),
                source: e,
            })?;
    }

    let output_data = if let Some(level) = compress {
        // Compress with oxipng
        let opts = match level {
            CompressionLevel::Level(n) => oxipng::Options::from_preset(n),
            CompressionLevel::Max => oxipng::Options::max_compression(),
        };
        oxipng::optimize_from_memory(&png_data.into_inner(), &opts).map_err(|e| {
            BentoError::PngCompress {
                path: path.to_path_buf(),
                message: e.to_string(),
            }
        })?
    } else {
        png_data.into_inner()
    };

    fs::write(path, output_data).map_err(|e| BentoError::OutputWrite {
        path: path.to_path_buf(),
        source: e,
    })?;

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
