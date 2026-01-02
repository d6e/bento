use image::{imageops::FilterType, ImageReader, RgbaImage};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Maximum thumbnail dimension (width or height)
pub const THUMBNAIL_SIZE: u32 = 24;

/// Load a single image and resize to thumbnail size
fn load_thumbnail(path: &Path) -> Option<RgbaImage> {
    let img = ImageReader::open(path).ok()?.decode().ok()?.into_rgba8();

    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }

    // Calculate scale to fit within THUMBNAIL_SIZE x THUMBNAIL_SIZE
    let scale = (THUMBNAIL_SIZE as f32 / w as f32).min(THUMBNAIL_SIZE as f32 / h as f32);

    let new_width = ((w as f32 * scale).round() as u32).max(1);
    let new_height = ((h as f32 * scale).round() as u32).max(1);

    Some(image::imageops::resize(
        &img,
        new_width,
        new_height,
        FilterType::Triangle,
    ))
}

/// Spawn background thread to load thumbnails for given paths
/// Returns receiver for results
pub fn spawn_thumbnail_loader(
    paths: Vec<PathBuf>,
) -> mpsc::Receiver<(PathBuf, Option<RgbaImage>)> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for path in paths {
            let image = load_thumbnail(&path);
            let _ = tx.send((path, image));
        }
    });

    rx
}
