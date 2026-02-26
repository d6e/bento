use image::{RgbaImage, imageops::FilterType};

/// Resize an image to a target width, preserving aspect ratio
pub fn resize_to_width(img: RgbaImage, target_width: u32, filter: FilterType) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = target_width as f32 / w as f32;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scale is positive, result fits in u32"
    )]
    let new_height = (h as f32 * scale).round() as u32;
    image::imageops::resize(&img, target_width, new_height.max(1), filter)
}

/// Resize an image by a scale factor
pub fn resize_by_scale(img: RgbaImage, scale: f32, filter: FilterType) -> RgbaImage {
    let (w, h) = img.dimensions();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scale is positive, result fits in u32"
    )]
    let new_width = (w as f32 * scale).round() as u32;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scale is positive, result fits in u32"
    )]
    let new_height = (h as f32 * scale).round() as u32;
    image::imageops::resize(
        &img,
        new_width.max(1),
        new_height.max(1),
        filter,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_resize_to_width_preserves_aspect_ratio() {
        let mut img = RgbaImage::new(200, 100);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let resized = resize_to_width(img, 100, FilterType::Lanczos3);

        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 50); // 100 * (100/200) = 50
    }

    #[test]
    fn test_resize_to_width_tall_image() {
        let mut img = RgbaImage::new(100, 400);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let resized = resize_to_width(img, 50, FilterType::Lanczos3);

        assert_eq!(resized.width(), 50);
        assert_eq!(resized.height(), 200); // 400 * (50/100) = 200
    }

    #[test]
    fn test_resize_by_scale_half() {
        let mut img = RgbaImage::new(100, 80);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let resized = resize_by_scale(img, 0.5, FilterType::Lanczos3);

        assert_eq!(resized.width(), 50);
        assert_eq!(resized.height(), 40);
    }

    #[test]
    fn test_resize_by_scale_double() {
        let mut img = RgbaImage::new(50, 30);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let resized = resize_by_scale(img, 2.0, FilterType::Lanczos3);

        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 60);
    }

    #[test]
    fn test_resize_minimum_dimension_is_one() {
        let mut img = RgbaImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        // Very small scale that would round to 0
        let resized = resize_by_scale(img, 0.001, FilterType::Lanczos3);

        assert!(resized.width() >= 1);
        assert!(resized.height() >= 1);
    }

    #[test]
    fn test_resize_with_nearest_filter() {
        let mut img = RgbaImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let resized = resize_by_scale(img, 0.5, FilterType::Nearest);

        assert_eq!(resized.width(), 50);
        assert_eq!(resized.height(), 50);
    }
}
