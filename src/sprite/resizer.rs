use image::{RgbaImage, imageops::FilterType};

/// Resize an image to a target width, preserving aspect ratio
pub fn resize_to_width(img: RgbaImage, target_width: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = target_width as f32 / w as f32;
    let new_height = (h as f32 * scale).round() as u32;
    image::imageops::resize(&img, target_width, new_height.max(1), FilterType::Lanczos3)
}

/// Resize an image by a scale factor
pub fn resize_by_scale(img: RgbaImage, scale: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let new_width = (w as f32 * scale).round() as u32;
    let new_height = (h as f32 * scale).round() as u32;
    image::imageops::resize(
        &img,
        new_width.max(1),
        new_height.max(1),
        FilterType::Lanczos3,
    )
}
