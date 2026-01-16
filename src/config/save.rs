use std::path::Path;

use anyhow::{Context, Result};

use super::types::BentoConfig;

/// Save a config to a JSON file with pretty formatting.
pub fn save_config(config: &BentoConfig, path: &Path) -> Result<()> {
    let content = serde_json::to_string_pretty(config)
        .with_context(|| "failed to serialize config to JSON")?;

    std::fs::write(path, content)
        .with_context(|| format!("failed to write config file: {}", path.display()))?;

    Ok(())
}

/// Convert an absolute path to a path relative to the base directory.
///
/// If the path cannot be made relative (e.g., different drive on Windows),
/// returns the original path as a string.
pub fn make_relative(path: &Path, base: &Path) -> String {
    // Try to strip the base prefix
    if let Ok(relative) = path.strip_prefix(base) {
        relative.to_string_lossy().into_owned()
    } else {
        // Fall back to the original path
        path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_make_relative_same_dir() {
        let path = PathBuf::from("/project/sprites/hero.png");
        let base = PathBuf::from("/project");
        assert_eq!(make_relative(&path, &base), "sprites/hero.png");
    }

    #[test]
    fn test_make_relative_nested() {
        let path = PathBuf::from("/project/assets/sprites/hero.png");
        let base = PathBuf::from("/project/assets");
        assert_eq!(make_relative(&path, &base), "sprites/hero.png");
    }

    #[test]
    fn test_make_relative_not_prefix() {
        let path = PathBuf::from("/other/sprites/hero.png");
        let base = PathBuf::from("/project");
        assert_eq!(make_relative(&path, &base), "/other/sprites/hero.png");
    }
}
