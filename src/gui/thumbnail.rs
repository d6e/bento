use image::{ImageReader, RgbaImage, imageops::FilterType};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Maximum thumbnail dimension (width or height)
pub const THUMBNAIL_SIZE: u32 = 24;

/// Load a single image and resize to thumbnail size
fn load_thumbnail(path: &Path) -> Option<RgbaImage> {
    // Test seam: a file named `panic_*` stands in for a decoder panic.
    #[cfg(test)]
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("panic_"))
    {
        #[allow(clippy::panic)]
        {
            panic!("simulated decoder panic");
        }
    }

    let img = ImageReader::open(path).ok()?.decode().ok()?.into_rgba8();

    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }

    // Calculate scale to fit within THUMBNAIL_SIZE x THUMBNAIL_SIZE
    let scale = (THUMBNAIL_SIZE as f32 / w as f32).min(THUMBNAIL_SIZE as f32 / h as f32);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let new_width = ((w as f32 * scale).round() as u32).max(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
pub fn spawn_thumbnail_loader(paths: Vec<PathBuf>) -> mpsc::Receiver<(PathBuf, Option<RgbaImage>)> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for path in paths {
            // A decoder panic must not end the thread: the receiver would see a
            // disconnect with this and later paths unanswered, and `poll_thumbnails`
            // would respawn a loader on them. Catch it and report the path as failed.
            // `AssertUnwindSafe` is fine because `path` is only read and the closure
            // captures nothing else (std::panic::catch_unwind / UnwindSafe docs).
            let image =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_thumbnail(&path)))
                    .unwrap_or(None);
            let _ = tx.send((path, image));
        }
    });

    rx
}
