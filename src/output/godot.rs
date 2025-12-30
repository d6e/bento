use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::atlas::Atlas;
use crate::error::BentoError;
use crate::sprite::PackedSprite;

/// Generate Godot .tres AtlasTexture files
pub fn write_godot_resources(
    atlases: &[Atlas],
    output_dir: &Path,
    base_name: &str,
    godot_res_path: Option<&str>,
) -> Result<()> {
    for atlas in atlases {
        let atlas_filename = format!("{}_{}.png", base_name, atlas.index);
        let res_path = godot_res_path
            .map(|p| format!("{}/{}", p.trim_end_matches('/'), atlas_filename))
            .unwrap_or_else(|| format!("res://{}", atlas_filename));

        for sprite in &atlas.sprites {
            let tres_path = output_dir.join(format!("{}.tres", sprite.name));
            let content = generate_tres(sprite, &res_path);

            fs::write(&tres_path, content).map_err(|e| BentoError::OutputWrite {
                path: tres_path,
                source: e,
            })?;
        }
    }

    Ok(())
}

fn generate_tres(sprite: &PackedSprite, atlas_path: &str) -> String {
    let (margin_left, margin_top, margin_right, margin_bottom) = sprite.trim_info.godot_margin();

    let has_margin =
        margin_left != 0 || margin_top != 0 || margin_right != 0 || margin_bottom != 0;

    let mut content = format!(
        r#"[gd_resource type="AtlasTexture" load_steps=2 format=3]

[ext_resource type="Texture2D" path="{}" id="1"]

[resource]
atlas = ExtResource("1")
region = Rect2({}, {}, {}, {})"#,
        atlas_path, sprite.x, sprite.y, sprite.width, sprite.height
    );

    if has_margin {
        content.push_str(&format!(
            "\nmargin = Rect2({}, {}, {}, {})",
            margin_left, margin_top, margin_right, margin_bottom
        ));
    }

    content.push_str("\nfilter_clip = true\n");

    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::TrimInfo;

    #[test]
    fn test_generate_tres_no_margin() {
        let sprite = PackedSprite {
            name: "test".to_string(),
            x: 10,
            y: 20,
            width: 32,
            height: 32,
            trim_info: TrimInfo::untrimmed(32, 32),
            atlas_index: 0,
        };

        let tres = generate_tres(&sprite, "res://atlas_0.png");

        assert!(tres.contains("region = Rect2(10, 20, 32, 32)"));
        assert!(!tres.contains("margin"));
        assert!(tres.contains("filter_clip = true"));
    }

    #[test]
    fn test_generate_tres_with_margin() {
        let sprite = PackedSprite {
            name: "test".to_string(),
            x: 10,
            y: 20,
            width: 28,
            height: 28,
            trim_info: TrimInfo {
                offset_x: 2,
                offset_y: 2,
                source_width: 32,
                source_height: 32,
                trimmed_width: 28,
                trimmed_height: 28,
            },
            atlas_index: 0,
        };

        let tres = generate_tres(&sprite, "res://atlas_0.png");

        assert!(tres.contains("region = Rect2(10, 20, 28, 28)"));
        assert!(tres.contains("margin = Rect2(2, 2, 2, 2)"));
    }
}
