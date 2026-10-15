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
        let dir = std::env::temp_dir().join(format!("bento_check_{}_{}", name, std::process::id()));
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
            .unwrap_or_else(|| {
                panic!(
                    "exit {}, no findings: {}",
                    output.status.code().unwrap(),
                    reports[0]
                )
            })
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

/// Rewrites every pixel of a sprite that is not fully transparent.
fn edit_sprite(project: &Project, name: &str, edit: impl Fn(Rgba<u8>) -> Rgba<u8>) {
    let path = project.sprite(name);
    let mut img = image::open(&path).unwrap().to_rgba8();
    for px in img.pixels_mut() {
        if px.0[3] != 0 {
            *px = edit(*px);
        }
    }
    img.save(&path).unwrap();
}

#[test]
fn test_small_colour_edit_is_reported_as_changed() {
    let project = Project::new("small_colour_edit", false);
    project.pack();
    edit_sprite(&project, "b", |Rgba([r, g, b, a])| {
        Rgba([
            r.saturating_add(10),
            g.saturating_add(10),
            b.saturating_add(10),
            a,
        ])
    });

    assert_eq!(
        project.check(),
        (1, vec![finding("changed", "sprites/b.png")])
    );
}

#[test]
fn test_alpha_only_edit_is_reported_as_changed() {
    let project = Project::new("alpha_only_edit", false);
    project.pack();
    edit_sprite(&project, "b", |Rgba([r, g, b, a])| {
        Rgba([r, g, b, if a == 255 { 225 } else { a }])
    });

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

/// A text merge of two packs can list one sprite twice. The source exists, so it is not orphaned.
#[test]
fn test_sprite_listed_twice_is_reported_as_broken_not_orphaned() {
    let project = Project::new("listed_twice", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let sprites = sheet["textures"][0]["sprites"].as_array_mut().unwrap();
        let copy = sprites[a].clone();
        sprites.push(copy);
    });

    assert_eq!(
        project.check(),
        (1, vec![finding("broken", "sprites/a.png")])
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

#[test]
fn test_margin_that_overflows_u32_is_broken_not_a_panic() {
    let project = Project::new("margin_overflow", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let margin = &mut sheet["textures"][0]["sprites"][a]["margin"];
        margin["w"] = u32::MAX.into();
        margin["h"] = u32::MAX.into();
    });

    assert_eq!(
        project.check(),
        (1, vec![finding("broken", "sprites/a.png")])
    );
}

#[test]
fn test_margin_offset_that_overflows_i32_is_broken_not_a_panic() {
    let project = Project::new("margin_offset_overflow", false);
    project.pack();
    project.edit_tpsheet(|sheet| {
        let a = sprite_index(sheet, "sprites/a.png");
        let margin = &mut sheet["textures"][0]["sprites"][a]["margin"];
        margin["x"] = i32::MAX.into();
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

    assert_eq!(output.status.code(), Some(2));
    let reports: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reports.as_array().unwrap().len(), 1);
    assert!(
        reports[0]["error"]
            .as_str()
            .unwrap()
            .contains("atlas.bento")
    );
    assert!(reports[0].get("findings").is_none());
}

#[test]
fn test_unchecked_config_does_not_hide_the_other_reports() {
    let good = Project::new("unchecked_good", false);
    good.pack();
    let unpacked = Project::new("unchecked_unpacked", false);

    let run = |json: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bento"));
        cmd.arg("check");
        if json {
            cmd.arg("--json");
        }
        cmd.arg(good.dir.join("atlas.bento"))
            .arg(unpacked.dir.join("atlas.bento"))
            .arg("/nonexistent/atlas.bento")
            .output()
            .unwrap()
    };

    let output = run(true);
    assert_eq!(output.status.code(), Some(2));
    let reports: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{}: {}", e, String::from_utf8_lossy(&output.stderr)));
    let reports = reports.as_array().unwrap();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0]["findings"].as_array().unwrap().len(), 0);
    assert!(reports[1]["error"].as_str().unwrap().contains("tpsheet"));
    assert!(reports[2]["error"].is_string());

    let output = run(false);
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("matches its sources"), "{}", text);
    assert!(text.contains("not checked"), "{}", text);
    assert!(text.contains("/nonexistent/atlas.bento"), "{}", text);
}

#[test]
fn test_unreadable_page_is_broken_and_its_sprites_are_not_missing() {
    let project = Project::new("unreadable_page", false);
    project.pack();
    let sheet: Value =
        serde_json::from_str(&std::fs::read_to_string(project.tpsheet()).unwrap()).unwrap();
    let image = sheet["textures"][0]["image"].as_str().unwrap();
    std::fs::write(
        project.dir.join("out").join(image),
        "version https://git-lfs.github.com/spec/v1\n",
    )
    .unwrap();

    assert_eq!(project.check(), (1, vec![("broken".to_string(), None)]));
}

#[test]
fn test_partial_alpha_colour_edit_is_reported_as_changed() {
    let project = Project::new("partial_alpha_edit", false);
    // No trim and no resize, so the source reaches the atlas pixel for pixel.
    std::fs::write(
        project.dir.join("atlas.bento"),
        r#"{"version": 1, "input": ["sprites/*.png"], "output_dir": "out", "name": "atlas",
            "trim_margin": 0, "padding": 2, "opaque": false, "compress": 2}"#,
    )
    .unwrap();
    let write = |colour: u8| {
        RgbaImage::from_pixel(8, 8, Rgba([colour, 0, 0, 128]))
            .save(project.sprite("a"))
            .unwrap();
    };
    write(100);
    project.pack();
    assert_eq!(project.check(), (0, vec![]));

    // 100 and 101 premultiply to the same u8 at alpha 128, but a repack changes the png.
    write(101);

    assert_eq!(
        project.check(),
        (1, vec![finding("changed", "sprites/a.png")])
    );
}

#[cfg(unix)]
#[test]
fn test_non_utf8_config_path_is_still_reported() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let project = Project::new("non_utf8_path", false);
    project.pack();
    let odd = project.dir.join(OsStr::from_bytes(b"odd\xff.bento"));
    std::fs::copy(project.dir.join("atlas.bento"), &odd).unwrap();
    let missing = project.dir.join(OsStr::from_bytes(b"gone\xfe.bento"));

    let run = |json: bool, configs: &[&Path]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bento"));
        cmd.arg("check");
        if json {
            cmd.arg("--json");
        }
        cmd.args(configs).output().unwrap()
    };

    // A clean atlas whose config path is not UTF-8: the report is printed and the exit is 0.
    let output = run(true, &[&odd]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(0), "{}", stderr);
    let reports: Value = serde_json::from_slice(&output.stdout).expect(&stderr);
    assert_eq!(reports.as_array().unwrap().len(), 1);
    assert_eq!(reports[0]["findings"].as_array().unwrap().len(), 0);
    assert!(reports[0]["config"].as_str().unwrap().contains("odd"));

    // An unchecked config with a non-UTF-8 path still exits 2 with valid JSON, next to the others.
    let output = run(true, &[&odd, &missing]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(2), "{}", stderr);
    let reports: Value = serde_json::from_slice(&output.stdout).expect(&stderr);
    assert_eq!(reports.as_array().unwrap().len(), 2);
    assert!(reports[1]["error"].is_string());

    // Text output has the same two outcomes.
    let output = run(false, &[&odd]);
    assert_eq!(output.status.code(), Some(0));
    let output = run(false, &[&odd, &missing]);
    assert_eq!(output.status.code(), Some(2));
}

/// A merge that leaves conflict markers in the tpsheet is merge damage to the atlas, not a
/// config the check could not run on.
#[test]
fn test_tpsheet_with_conflict_markers_is_reported_as_broken() {
    let project = Project::new("conflict_markers", false);
    project.pack();
    let text = std::fs::read_to_string(project.tpsheet()).unwrap();
    std::fs::write(project.tpsheet(), format!("<<<<<<< HEAD\n{}", text)).unwrap();

    let (code, findings) = project.check();
    assert_eq!((code, findings), (1, vec![("broken".to_string(), None)]));
}

#[test]
fn test_tpsheet_with_non_utf8_bytes_is_reported_as_broken() {
    let project = Project::new("tpsheet_non_utf8", false);
    project.pack();
    std::fs::write(project.tpsheet(), [0xff, 0xfe, 0x00, 0x80]).unwrap();

    let (code, findings) = project.check();
    assert_eq!((code, findings), (1, vec![("broken".to_string(), None)]));
}
