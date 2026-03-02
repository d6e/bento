use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use image::ImageReader;
use log::info;
use rayon::prelude::*;

use super::{SourceSprite, TrimInfo, resize_by_scale, resize_to_width, trim_sprite};
use crate::cli::ResizeFilter;
use crate::error::BentoError;

const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Image path with its base directory for computing relative paths
struct ImagePath {
    path: std::path::PathBuf,
    base: Option<std::path::PathBuf>,
}

/// Load sprites from input paths (files or directories)
///
/// When `base_dir` is provided, individual file inputs will have their sprite
/// names computed as paths relative to that directory. This preserves subdirectory
/// structure in output metadata (e.g., "ironclad/bash.png" instead of "bash.png").
/// Config-file loading uses this to pass the config directory as the base.
///
/// When `filename_only` is true, all sprites use bare filenames regardless of
/// directory structure or `base_dir`.
pub fn load_sprites(
    inputs: &[impl AsRef<Path>],
    trim: bool,
    trim_margin: u32,
    resize_width: Option<u32>,
    resize_scale: Option<f32>,
    resize_filter: ResizeFilter,
    cancel_token: Option<&Arc<AtomicBool>>,
    base_dir: Option<&Path>,
    filename_only: bool,
) -> Result<Vec<SourceSprite>> {
    let image_paths = collect_image_paths(inputs, base_dir, filename_only)?;

    if image_paths.is_empty() {
        return Err(BentoError::NoImages.into());
    }

    info!("Loading {} images...", image_paths.len());

    let sprites: Result<Vec<_>> = image_paths
        .par_iter()
        .map(|img_path| {
            // Check for cancellation before loading each image
            if let Some(token) = cancel_token
                && token.load(Ordering::Relaxed)
            {
                return Err(BentoError::Cancelled.into());
            }
            load_single_sprite(
                &img_path.path,
                img_path.base.as_deref(),
                trim,
                trim_margin,
                resize_width,
                resize_scale,
                resize_filter,
            )
        })
        .collect();

    let mut sprites = sprites?;

    // Check for duplicate sprite names (would cause silent overwrites in Godot output)
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for sprite in &sprites {
        *name_counts.entry(&sprite.name).or_insert(0) += 1;
    }
    let duplicates: Vec<&str> = name_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name)
        .collect();
    if !duplicates.is_empty() {
        let mut sorted = duplicates;
        sorted.sort_unstable();
        return Err(BentoError::DuplicateNames {
            names: sorted.join(", "),
        }
        .into());
    }

    sprites.sort_by(|a, b| {
        // Sort by area descending for better packing
        let area_a = u64::from(a.width()) * u64::from(a.height());
        let area_b = u64::from(b.width()) * u64::from(b.height());
        area_b.cmp(&area_a)
    });

    Ok(sprites)
}

fn collect_image_paths(
    inputs: &[impl AsRef<Path>],
    base_dir: Option<&Path>,
    filename_only: bool,
) -> Result<Vec<ImagePath>> {
    let mut paths = Vec::new();

    for input in inputs {
        let path = input.as_ref();
        if !path.exists() {
            return Err(BentoError::InputNotFound(path.to_path_buf()).into());
        }

        if path.is_file() {
            if is_supported_image(path) {
                paths.push(ImagePath {
                    path: path.to_path_buf(),
                    base: if filename_only {
                        None
                    } else {
                        base_dir.map(Path::to_path_buf)
                    },
                });
            }
        } else if path.is_dir() {
            collect_from_directory(path, path, filename_only, &mut paths)?;
        }
    }

    Ok(paths)
}

fn collect_from_directory(
    base: &Path,
    dir: &Path,
    filename_only: bool,
    paths: &mut Vec<ImagePath>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && is_supported_image(&path) {
            paths.push(ImagePath {
                path,
                base: if filename_only {
                    None
                } else {
                    Some(base.to_path_buf())
                },
            });
        } else if path.is_dir() {
            collect_from_directory(base, &path, filename_only, paths)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ResizeFilter;

    /// Create a minimal valid 1x1 PNG file.
    fn write_test_png(path: &Path) {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        img.save(path).expect("failed to write test png");
    }

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bento_test_{}", name));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
        }
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn test_filename_only_strips_directory_for_file_inputs() {
        let dir = make_temp_dir("fo_file");
        let sub = dir.join("enemies");
        std::fs::create_dir_all(&sub).expect("mkdir");
        write_test_png(&sub.join("bat.png"));

        // With base_dir and filename_only=false, name preserves relative path
        let sprites = load_sprites(
            &[sub.join("bat.png")],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            Some(dir.as_path()),
            false,
        ).expect("load ok");
        assert_eq!(sprites[0].name, "enemies/bat.png");

        // With filename_only=true, name is bare filename
        let sprites = load_sprites(
            &[sub.join("bat.png")],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            Some(dir.as_path()),
            true,
        ).expect("load ok");
        assert_eq!(sprites[0].name, "bat.png");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_filename_only_strips_directory_for_dir_inputs() {
        let dir = make_temp_dir("fo_dir");
        let sub = dir.join("units");
        std::fs::create_dir_all(&sub).expect("mkdir");
        write_test_png(&sub.join("hero.png"));

        // Without filename_only, directory input preserves relative path
        let sprites = load_sprites(
            &[dir.clone()],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            None,
            false,
        ).expect("load ok");
        assert_eq!(sprites[0].name, "units/hero.png");

        // With filename_only, bare filename
        let sprites = load_sprites(
            &[dir.clone()],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            None,
            true,
        ).expect("load ok");
        assert_eq!(sprites[0].name, "hero.png");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_duplicate_names_detected() {
        let dir = make_temp_dir("fo_dup");
        let a = dir.join("a");
        let b = dir.join("b");
        std::fs::create_dir_all(&a).expect("mkdir");
        std::fs::create_dir_all(&b).expect("mkdir");
        write_test_png(&a.join("icon.png"));
        write_test_png(&b.join("icon.png"));

        // filename_only causes both to be named "icon.png" -> error
        let result = load_sprites(
            &[a.join("icon.png"), b.join("icon.png")],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            None,
            true,
        );
        let err = result.expect_err("should fail on duplicates");
        let msg = err.to_string();
        assert!(msg.contains("icon.png"), "error should mention the duplicate name: {msg}");
        assert!(msg.contains("Duplicate"), "error should mention 'Duplicate': {msg}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_no_duplicate_error_when_names_unique() {
        let dir = make_temp_dir("fo_uniq");
        write_test_png(&dir.join("alpha.png"));
        write_test_png(&dir.join("beta.png"));

        let result = load_sprites(
            &[dir.join("alpha.png"), dir.join("beta.png")],
            false, 0, None, None,
            ResizeFilter::Nearest,
            None,
            None,
            false,
        );
        assert!(result.is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }
}

fn load_single_sprite(
    path: &Path,
    base: Option<&Path>,
    trim: bool,
    trim_margin: u32,
    resize_width: Option<u32>,
    resize_scale: Option<f32>,
    resize_filter: ResizeFilter,
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
    let filter = resize_filter.to_image_filter();
    let img = match (resize_width, resize_scale) {
        (Some(w), None) => resize_to_width(img, w, filter),
        (None, Some(s)) => resize_by_scale(img, s, filter),
        _ => img,
    };

    // Compute sprite name: relative path with extension for directory inputs,
    // or filename with extension for individual file inputs
    let name = match base {
        Some(base_dir) => {
            // Compute relative path from base directory
            path.strip_prefix(base_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string()
        }
        None => {
            // Individual file: use filename with extension
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        }
    };

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
