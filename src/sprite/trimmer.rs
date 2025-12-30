use image::RgbaImage;

use super::TrimInfo;

/// Trim transparent borders from an image
pub fn trim_sprite(image: &RgbaImage) -> (RgbaImage, TrimInfo) {
    let (width, height) = image.dimensions();

    if width == 0 || height == 0 {
        return (
            RgbaImage::new(1, 1),
            TrimInfo {
                offset_x: 0,
                offset_y: 0,
                source_width: width,
                source_height: height,
                trimmed_width: 1,
                trimmed_height: 1,
            },
        );
    }

    // Find bounding box of non-transparent pixels
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x, y);
            if pixel[3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    // Handle fully transparent image
    if max_x < min_x || max_y < min_y {
        return (
            RgbaImage::new(1, 1),
            TrimInfo {
                offset_x: 0,
                offset_y: 0,
                source_width: width,
                source_height: height,
                trimmed_width: 1,
                trimmed_height: 1,
            },
        );
    }

    let trimmed_width = max_x - min_x + 1;
    let trimmed_height = max_y - min_y + 1;

    let trimmed = image::imageops::crop_imm(image, min_x, min_y, trimmed_width, trimmed_height)
        .to_image();

    let trim_info = TrimInfo {
        offset_x: min_x as i32,
        offset_y: min_y as i32,
        source_width: width,
        source_height: height,
        trimmed_width,
        trimmed_height,
    };

    (trimmed, trim_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_trim_fully_opaque() {
        let mut img = RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }

        let (trimmed, info) = trim_sprite(&img);

        assert_eq!(trimmed.width(), 10);
        assert_eq!(trimmed.height(), 10);
        assert_eq!(info.offset_x, 0);
        assert_eq!(info.offset_y, 0);
        assert!(!info.was_trimmed());
    }

    #[test]
    fn test_trim_with_transparent_border() {
        let mut img = RgbaImage::new(10, 10);
        // Fill center 4x4 with opaque pixels
        for y in 3..7 {
            for x in 2..6 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }

        let (trimmed, info) = trim_sprite(&img);

        assert_eq!(trimmed.width(), 4);
        assert_eq!(trimmed.height(), 4);
        assert_eq!(info.offset_x, 2);
        assert_eq!(info.offset_y, 3);
        assert_eq!(info.source_width, 10);
        assert_eq!(info.source_height, 10);
        assert!(info.was_trimmed());
    }

    #[test]
    fn test_trim_fully_transparent() {
        let img = RgbaImage::new(10, 10);

        let (trimmed, info) = trim_sprite(&img);

        assert_eq!(trimmed.width(), 1);
        assert_eq!(trimmed.height(), 1);
        assert_eq!(info.source_width, 10);
        assert_eq!(info.source_height, 10);
    }

    #[test]
    fn test_godot_margin() {
        let info = TrimInfo {
            offset_x: 2,
            offset_y: 3,
            source_width: 10,
            source_height: 10,
            trimmed_width: 4,
            trimmed_height: 4,
        };

        let (left, top, right, bottom) = info.godot_margin();
        assert_eq!(left, 2);
        assert_eq!(top, 3);
        assert_eq!(right, 4); // 10 - 4 - 2 = 4
        assert_eq!(bottom, 3); // 10 - 4 - 3 = 3
    }
}
