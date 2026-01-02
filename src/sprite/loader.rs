use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use image::ImageReader;
use log::info;
use rayon::prelude::*;

use super::{SourceSprite, TrimInfo, resize_by_scale, resize_to_width, trim_sprite};
use crate::error::BentoError;

const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Load sprites from input paths (files or directories)
pub fn load_sprites(
    inputs: &[impl AsRef<Path>],
    trim: bool,
    trim_margin: u32,
    resize_width: Option<u32>,
    resize_scale: Option<f32>,
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<Vec<SourceSprite>> {
    let image_paths = collect_image_paths(inputs)?;

    if image_paths.is_empty() {
        return Err(BentoError::NoImages.into());
    }

    info!("Loading {} images...", image_paths.len());

    let sprites: Result<Vec<_>> = image_paths
        .par_iter()
        .map(|path| {
            // Check for cancellation before loading each image
            if let Some(token) = cancel_token
                && token.load(Ordering::Relaxed)
            {
                return Err(BentoError::Cancelled.into());
            }
            load_single_sprite(path, trim, trim_margin, resize_width, resize_scale)
        })
        .collect();

    let mut sprites = sprites?;
    sprites.sort_by(|a, b| {
        // Sort by area descending for better packing
        let area_a = u64::from(a.width()) * u64::from(a.height());
        let area_b = u64::from(b.width()) * u64::from(b.height());
        area_b.cmp(&area_a)
    });

    Ok(sprites)
}

fn collect_image_paths(inputs: &[impl AsRef<Path>]) -> Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();

    for input in inputs {
        let path = input.as_ref();
        if !path.exists() {
            return Err(BentoError::InputNotFound(path.to_path_buf()).into());
        }

        if path.is_file() {
            if is_supported_image(path) {
                paths.push(path.to_path_buf());
            }
        } else if path.is_dir() {
            collect_from_directory(path, &mut paths)?;
        }
    }

    Ok(paths)
}

fn collect_from_directory(dir: &Path, paths: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && is_supported_image(&path) {
            paths.push(path);
        } else if path.is_dir() {
            collect_from_directory(&path, paths)?;
        }
    }

    Ok(())
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn load_single_sprite(
    path: &Path,
    trim: bool,
    trim_margin: u32,
    resize_width: Option<u32>,
    resize_scale: Option<f32>,
) -> Result<SourceSprite> {
    let img = ImageReader::open(path)
        .map_err(|e| BentoError::ImageLoad {
            path: path.to_path_buf(),
            source: e.into(),
        })?
        .decode()
        .map_err(|e| BentoError::ImageLoad {
            path: path.to_path_buf(),
            source: e,
        })?
        .into_rgba8();

    // Resize if requested (before trimming)
    let img = match (resize_width, resize_scale) {
        (Some(w), None) => resize_to_width(img, w),
        (None, Some(s)) => resize_by_scale(img, s),
        _ => img,
    };

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (image, trim_info) = if trim {
        trim_sprite(&img, trim_margin)
    } else {
        let (w, h) = img.dimensions();
        (img, TrimInfo::untrimmed(w, h))
    };

    Ok(SourceSprite {
        path: path.to_path_buf(),
        name,
        image,
        trim_info,
    })
}
