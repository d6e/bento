//! `bento check`: compare a packed atlas with its config's sources without repacking.
//!
//! It answers two questions that need different handling. Is the atlas broken, meaning its
//! tpsheet no longer describes its png (a page size, an out-of-bounds region, or a pixel hash
//! that does not match)? That is always a bug. And is a repack due, meaning sources were added,
//! removed or edited since the last pack? That is normal between packs.
//!
//! It compares sprites only. A config change that a repack would apply (padding, extrude,
//! block_align, pot, size limits, opaque, trimming) is not detected.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use image::{RgbaImage, imageops};
use serde::Serialize;

use crate::cli::ResizeFilter;
use crate::config::{LoadedConfig, ResizeConfig};
use crate::output::pixel_hash;
use crate::output::tpsheet::{TpSprite, TpsheetOutput};
use crate::sprite::{SourceSprite, load_sprites};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// The tpsheet does not describe the png. Repack.
    Broken,
    /// The atlas holds different pixels from the sprite's source image.
    Changed,
    /// A source image is not in the atlas.
    Missing,
    /// The atlas holds a sprite with no source image.
    Orphaned,
    /// Sprites without a pixel hash (TexturePacker, or bento before 0.7), so `broken` cannot
    /// be ruled out for them.
    Unhashed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub kind: FindingKind,
    /// Sprite name, or None for a finding about a whole page.
    pub sprite: Option<String>,
    pub detail: String,
}

/// Serialises a path as text, replacing invalid UTF-8 so a report is never lost to its own path.
pub fn serialize_path_lossy<S: serde::Serializer>(path: &Path, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&path.to_string_lossy())
}

#[derive(Debug, Clone, Serialize)]
pub struct AtlasReport {
    pub atlas: String,
    #[serde(serialize_with = "serialize_path_lossy")]
    pub config: PathBuf,
    pub findings: Vec<Finding>,
}

impl AtlasReport {
    pub fn count(&self, kind: FindingKind) -> usize {
        self.findings.iter().filter(|f| f.kind == kind).count()
    }
}

/// Check the tpsheet atlas a `.bento` config produces against that config's sources.
pub fn check_atlas(config_path: &Path) -> Result<AtlasReport> {
    let loaded = LoadedConfig::load(config_path)
        .with_context(|| format!("failed to load config: {}", config_path.display()))?;
    let config = &loaded.config;
    let output_dir = loaded.resolve_output_dir();
    let tpsheet_path = output_dir.join(format!("{}.tpsheet", config.name));
    let tpsheet: TpsheetOutput = serde_json::from_str(
        &std::fs::read_to_string(&tpsheet_path)
            .with_context(|| format!("failed to read {}", tpsheet_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", tpsheet_path.display()))?;

    let sources = load_config_sources(&loaded)?;
    let mut by_name: HashMap<&str, &SourceSprite> =
        sources.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut findings = Vec::new();
    let mut listed: HashSet<&str> = HashSet::new();
    for page in &tpsheet.textures {
        let png_path = output_dir.join(&page.image);
        let png = match image::open(&png_path) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                findings.push(page_finding(format!("cannot read {}: {}", page.image, e)));
                // The sprites are listed, just unchecked, so they are not missing.
                for sprite in &page.sprites {
                    listed.insert(sprite.filename.as_str());
                    by_name.remove(sprite.filename.as_str());
                }
                continue;
            }
        };

        if (png.width(), png.height()) != (page.size.w, page.size.h) {
            findings.push(page_finding(format!(
                "{} is {}x{} but the tpsheet says {}x{}",
                page.image,
                png.width(),
                png.height(),
                page.size.w,
                page.size.h
            )));
        }

        let mut unhashed = 0;
        for sprite in &page.sprites {
            // The first entry keeps the source and is judged on its own; only the repeats are
            // malformed, since the tpsheet cannot say which of the two a pack wrote.
            if !listed.insert(sprite.filename.as_str()) {
                findings.push(sprite_finding(
                    FindingKind::Broken,
                    sprite,
                    "listed more than once in the tpsheet".to_string(),
                ));
                continue;
            }
            let source = by_name.remove(sprite.filename.as_str());
            if let Some(broken) = check_sprite_integrity(&png, sprite) {
                findings.push(sprite_finding(FindingKind::Broken, sprite, broken));
                continue;
            }
            if sprite.pixel_hash.is_none() {
                unhashed += 1;
            }
            match source {
                None => findings.push(sprite_finding(
                    FindingKind::Orphaned,
                    sprite,
                    "no source image".to_string(),
                )),
                Some(source) => {
                    if let Some(change) = compare_to_source(&png, sprite, source, config.opaque) {
                        findings.push(sprite_finding(FindingKind::Changed, sprite, change));
                    }
                }
            }
        }
        if unhashed > 0 {
            findings.push(Finding {
                kind: FindingKind::Unhashed,
                sprite: None,
                detail: format!(
                    "{} of {} sprites on {}",
                    unhashed,
                    page.sprites.len(),
                    page.image
                ),
            });
        }
    }

    let mut missing: Vec<&str> = by_name.into_keys().collect();
    missing.sort_unstable();
    findings.extend(missing.into_iter().map(|name| Finding {
        kind: FindingKind::Missing,
        sprite: Some(name.to_string()),
        detail: "not in the atlas".to_string(),
    }));

    Ok(AtlasReport {
        atlas: config.name.clone(),
        config: config_path.to_path_buf(),
        findings,
    })
}

fn load_config_sources(loaded: &LoadedConfig) -> Result<Vec<SourceSprite>> {
    let config = &loaded.config;
    let filter = ResizeFilter::from_str(&config.resize_filter, true)
        .map_err(|e| anyhow::anyhow!("unknown resize_filter '{}': {}", config.resize_filter, e))?;
    let (resize_width, resize_scale) = match &config.resize {
        Some(ResizeConfig::Width { width }) => (Some(*width), None),
        Some(ResizeConfig::Scale { scale }) => (None, Some(*scale)),
        None => (None, None),
    };
    let inputs = loaded
        .resolve_inputs()
        .context("failed to resolve input files from config")?;

    load_sprites(
        &inputs,
        config.trim,
        config.trim_margin,
        resize_width,
        resize_scale,
        filter,
        None,
        Some(&loaded.config_dir),
        config.filename_only,
    )
}

/// Returns why the tpsheet entry does not describe this png, if it does not.
fn check_sprite_integrity(png: &RgbaImage, sprite: &TpSprite) -> Option<String> {
    let r = &sprite.region;
    let fits =
        |origin: u32, len: u32, page: u32| origin.checked_add(len).is_some_and(|end| end <= page);
    if !fits(r.x, r.w, png.width()) || !fits(r.y, r.h, png.height()) {
        return Some(format!(
            "region ({}, {}, {}x{}) runs past the edge of the page",
            r.x, r.y, r.w, r.h
        ));
    }
    let m = &sprite.margin;
    let sized = r.w.checked_add(m.w).is_some() && r.h.checked_add(m.h).is_some();
    let inside = |offset: i32, slack: u32| u32::try_from(offset).is_ok_and(|o| o <= slack);
    if !sized || !inside(m.x, m.w) || !inside(m.y, m.h) {
        return Some(format!(
            "margin ({}, {}, {}x{}) is not one a pack could have written for a {}x{} region",
            m.x, m.y, m.w, m.h, r.w, r.h
        ));
    }
    let expected = sprite.pixel_hash.as_ref()?;
    let actual = pixel_hash(png, r.x, r.y, r.w, r.h, false);
    (&actual != expected).then(|| {
        format!(
            "pixel hash {} does not match the png ({}), so the tpsheet and png are from different packs",
            expected, actual
        )
    })
}

/// Compares the sprite as packed with its source as a pack would process it now, on the
/// untrimmed canvas so a one-pixel difference in trimming does not count as a change.
fn compare_to_source(
    png: &RgbaImage,
    sprite: &TpSprite,
    source: &SourceSprite,
    opaque: bool,
) -> Option<String> {
    let r = &sprite.region;
    let m = &sprite.margin;
    let trim = &source.trim_info;
    let packed_size = (r.w + m.w, r.h + m.h);
    let source_size = (trim.source_width, trim.source_height);
    if packed_size != source_size {
        return Some(format!(
            "{}x{} in the atlas, {}x{} from the source",
            packed_size.0, packed_size.1, source_size.0, source_size.1
        ));
    }

    let mut packed = RgbaImage::new(packed_size.0, packed_size.1);
    for y in 0..r.h {
        for x in 0..r.w {
            let px = *png.get_pixel(r.x + x, r.y + y);
            put(&mut packed, m.x + x as i32, m.y + y as i32, px);
        }
    }
    // Draw the source the way the packer does, blended onto a transparent canvas: a fully
    // transparent source pixel lands as (0, 0, 0, 0), not as its stored colour.
    let mut current = RgbaImage::new(source_size.0, source_size.1);
    imageops::overlay(
        &mut current,
        &source.image,
        i64::from(trim.offset_x),
        i64::from(trim.offset_y),
    );
    if opaque {
        // An opaque atlas is saved without alpha, so the packed copy reads back as 255.
        for px in packed.pixels_mut().chain(current.pixels_mut()) {
            px.0[3] = 255;
        }
    }

    let distances: Vec<u8> = packed
        .pixels()
        .zip(current.pixels())
        .map(|(a, b)| pixel_distance(a.0, b.0))
        .collect();
    let changed = distances.iter().filter(|d| **d > 0).count();
    let largest = distances.iter().copied().max().unwrap_or(0);
    (changed > 0).then(|| {
        format!(
            "{} of {} pixels differ from the source, by up to {}",
            changed,
            packed_size.0 * packed_size.1,
            largest
        )
    })
}

fn put(image: &mut RgbaImage, x: i32, y: i32, px: image::Rgba<u8>) {
    if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y))
        && x < image.width()
        && y < image.height()
    {
        image.put_pixel(x, y, px);
    }
}

/// Largest per-channel difference between two pixels, compared the way `pixel_hash` does: a
/// fully transparent pixel is (0, 0, 0, 0) and any other pixel is compared as stored.
fn pixel_distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    let normalise = |px: [u8; 4]| if px[3] == 0 { [0; 4] } else { px };
    let (a, b) = (normalise(a), normalise(b));
    (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0)
}

fn page_finding(detail: String) -> Finding {
    Finding {
        kind: FindingKind::Broken,
        sprite: None,
        detail,
    }
}

fn sprite_finding(kind: FindingKind, sprite: &TpSprite, detail: String) -> Finding {
    Finding {
        kind,
        sprite: Some(sprite.filename.clone()),
        detail,
    }
}
