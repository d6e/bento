use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BentoError {
    #[error("Failed to load image '{path}': {source}")]
    ImageLoad {
        path: PathBuf,
        source: image::ImageError,
    },

    #[error("Failed to save image '{path}': {source}")]
    ImageSave {
        path: PathBuf,
        source: image::ImageError,
    },

    #[error("No valid images found in input")]
    NoImages,

    #[error(
        "Sprite '{name}' ({width}x{height}) exceeds maximum atlas size ({max_width}x{max_height})"
    )]
    SpriteTooLarge {
        name: String,
        width: u32,
        height: u32,
        max_width: u32,
        max_height: u32,
    },

    #[error("Failed to write output file '{path}': {source}")]
    OutputWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to compress PNG '{path}': {message}")]
    PngCompress { path: PathBuf, message: String },

    #[error("Input path does not exist: {0}")]
    InputNotFound(PathBuf),
}
