mod format;
mod godot;
mod json;
mod tpsheet;

pub use format::save_atlas_image;
pub use godot::write_godot_resources;
pub use json::write_json;
pub use tpsheet::write_tpsheet;

/// Returns the PNG filename for an atlas. Single-atlas packs use `{name}.png`,
/// multi-atlas packs use `{name}_{index}.png`.
pub fn atlas_png_filename(base_name: &str, index: usize, total: usize) -> String {
    if total == 1 {
        format!("{}.png", base_name)
    } else {
        format!("{}_{}.png", base_name, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_atlas_no_suffix() {
        assert_eq!(atlas_png_filename("power_atlas", 0, 1), "power_atlas.png");
    }

    #[test]
    fn test_multi_atlas_has_suffix() {
        assert_eq!(atlas_png_filename("card_atlas", 0, 3), "card_atlas_0.png");
        assert_eq!(atlas_png_filename("card_atlas", 1, 3), "card_atlas_1.png");
        assert_eq!(atlas_png_filename("card_atlas", 2, 3), "card_atlas_2.png");
    }

    #[test]
    fn test_two_atlases_has_suffix() {
        assert_eq!(atlas_png_filename("atlas", 0, 2), "atlas_0.png");
        assert_eq!(atlas_png_filename("atlas", 1, 2), "atlas_1.png");
    }
}
