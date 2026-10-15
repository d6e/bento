//! `bento check` against atlases packed by the real binary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use image::{Rgba, RgbaImage};
use serde_json::Value;

struct Project {
    dir: PathBuf,
}

impl Project {
    /// Sprites with transparent borders (so trimming applies), partial alpha, and fully
    /// transparent pixels holding stray colour, which an opaque atlas must not compare on.
    fn new(name: &str, opaque: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("bento_check_{}", name));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(dir.join("sprites")).unwrap();
        std::fs::write(
            dir.join("atlas.bento"),
            format!(
                r#"{{"version": 1, "input": ["sprites/*.png"], "output_dir": "out", "name": "atlas",
                    "trim_margin": 1, "padding": 2, "resize": {{"width": 24}}, "opaque": {}, "compress": 2}}"#,
                opaque
            ),
        )
        .unwrap();
        let project = Self { dir };
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            project.write_sprite(name, i as u8 * 70);
        }
        project
    }

    fn write_sprite(&self, name: &str, seed: u8) {
        let img = RgbaImage::from_fn(32, 32, |x, y| {
            if !(4..28).contains(&x) || !(4..28).contains(&y) {
                Rgba([255, 0, 255, 0])
            } else {
                let alpha = if x == 4 || y == 4 { 128 } else { 255 };
                Rgba([seed.wrapping_add((x * 7) as u8), (y * 9) as u8, seed, alpha])
            }
        });
        img.save(self.sprite(name)).unwrap();
    }

    fn sprite(&self, name: &str) -> PathBuf {
        self.dir.join("sprites").join(format!("{}.png", name))
    }

    fn tpsheet(&self) -> PathBuf {
        self.dir.join("out").join("atlas.tpsheet")
    }

    fn pack(&self) {
        let status = Command::new(env!("CARGO_BIN_EXE_bento"))
            .args(["tpsheet", "--config"])
            .arg(self.dir.join("atlas.bento"))
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// (exit code, findings as (kind, sprite) pairs)
    fn check(&self) -> (i32, Vec<(String, Option<String>)>) {
        let output = Command::new(env!("CARGO_BIN_EXE_bento"))
            .args(["check", "--json"])
            .arg(self.dir.join("atlas.bento"))
            .output()
            .unwrap();
        let reports: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{}: {}", e, String::from_utf8_lossy(&output.stderr)));
        let findings = reports[0]["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["kind"].as_str().unwrap().to_string(),
                    f["sprite"].as_str().map(str::to_string),
                )
            })
            .collect();
        (output.status.code().unwrap(), findings)
    }

    fn edit_tpsheet(&self, edit: impl FnOnce(&mut Value)) {
        let mut sheet: Value =
            serde_json::from_str(&std::fs::read_to_string(self.tpsheet()).unwrap()).unwrap();
        edit(&mut sheet);
        std::fs::write(
            self.tpsheet(),
            serde_json::to_string_pretty(&sheet).unwrap(),
        )
        .unwrap();
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn sprite_index(sheet: &Value, name: &str) -> usize {
    sheet["textures"][0]["sprites"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["filename"] == name)
        .unwrap()
}

fn finding(kind: &str, sprite: &str) -> (String, Option<String>) {
    (kind.to_string(), Some(sprite.to_string()))
}

#[test]
fn test_fresh_pack_matches_its_sources() {
    for opaque in [false, true] {
        let project = Project::new(&format!("fresh_{}", opaque), opaque);
        project.pack();

        assert_eq!(project.check(), (0, vec![]), "opaque={}", opaque);
    }
}

#[test]
fn test_edited_source_is_reported_as_changed() {
    let project = Project::new("edited", false);
    project.pack();
    project.write_sprite("b", 200);

    assert_eq!(
        project.check(),
        (1, vec![finding("changed", "sprites/b.png")])
    );
}

#[test]
fn test_added_and_deleted_sources_are_reported() {
    let project = Project::new("added_deleted", false);
    project.pack();
    project.write_sprite("d", 30);
    std::fs::remove_file(project.sprite("a")).unwrap();

    let (code, mut findings) = project.check();
    findings.sort();
    assert_eq!(code, 1);
    assert_eq!(
        findings,
        vec![
            finding("missing", "sprites/d.png"),
            finding("orphaned", "sprites/a.png")
        ]
    );
}

/// What a text merge of two packs leaves: sprite entries pointing at another sprite's pixels.
#[test]
fn test_regions_from_another_pack_are_reported_as_broken() {
    let project = Project::new("swapped", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let b = sprite_index(sheet, "sprites/b.png");
        let sprites = sheet["textures"][0]["sprites"].as_array_mut().unwrap();
        let region_a = sprites[a]["region"].clone();
        sprites[a]["region"] = sprites[b]["region"].clone();
        sprites[b]["region"] = region_a;
    });

    let (code, mut findings) = project.check();
    findings.sort();
    assert_eq!(code, 1);
    assert_eq!(
        findings,
        vec![
            finding("broken", "sprites/a.png"),
            finding("broken", "sprites/b.png")
        ]
    );
}

#[test]
fn test_page_size_that_does_not_match_the_png_is_broken() {
    let project = Project::new("page_size", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let w = sheet["textures"][0]["size"]["w"].as_u64().unwrap();
        sheet["textures"][0]["size"]["w"] = (w + 4).into();
    });

    assert_eq!(project.check(), (1, vec![("broken".to_string(), None)]));
}

#[test]
fn test_region_past_the_page_edge_is_broken() {
    let project = Project::new("out_of_bounds", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let w = sheet["textures"][0]["size"]["w"].as_u64().unwrap();
        sheet["textures"][0]["sprites"][a]["region"]["x"] = w.into();
    });

    assert_eq!(
        project.check(),
        (1, vec![finding("broken", "sprites/a.png")])
    );
}

#[test]
fn test_region_that_overflows_u32_is_broken_not_a_panic() {
    let project = Project::new("region_overflow", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let region = &mut sheet["textures"][0]["sprites"][a]["region"];
        region["x"] = u32::MAX.into();
        region["w"] = 2.into();
    });

    assert_eq!(
        project.check(),
        (1, vec![finding("broken", "sprites/a.png")])
    );
}

/// TexturePacker and older bento write no hashes; check still compares sources but says so.
#[test]
fn test_tpsheet_without_hashes_is_reported_as_unhashed() {
    let project = Project::new("unhashed", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        for sprite in sheet["textures"][0]["sprites"].as_array_mut().unwrap() {
            sprite.as_object_mut().unwrap().remove("pixel_hash");
        }
    });

    assert_eq!(project.check(), (1, vec![("unhashed".to_string(), None)]));
}

#[test]
fn test_missing_config_is_an_error_not_a_clean_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_bento"))
        .args(["check", "--json"])
        .arg(Path::new("/nonexistent/atlas.bento"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
