use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::types::BentoConfig;

/// A loaded configuration file with its associated directory.
///
/// Paths in the config are relative to the config file location,
/// so we need to track where the config was loaded from.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// The parsed configuration
    pub config: BentoConfig,
    /// The directory containing the config file
    pub config_dir: PathBuf,
}

impl LoadedConfig {
    /// Load a config file from the given path.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        let config: BentoConfig = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;

        let config_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        Ok(Self { config, config_dir })
    }

    /// Resolve input patterns to actual file paths.
    ///
    /// Glob patterns are expanded, and all paths are resolved relative
    /// to the config file directory.
    pub fn resolve_inputs(&self) -> Result<Vec<PathBuf>> {
        let mut results = Vec::new();

        for pattern in &self.config.input {
            if is_glob_pattern(pattern) {
                // Resolve glob pattern relative to config dir
                let full_pattern = self.config_dir.join(pattern);
                let pattern_str = full_pattern.to_string_lossy();

                let paths = glob::glob(&pattern_str)
                    .with_context(|| format!("invalid glob pattern: {}", pattern))?;

                for entry in paths {
                    let path =
                        entry.with_context(|| format!("failed to read glob entry: {}", pattern))?;
                    results.push(path);
                }
            } else {
                // Regular path, resolve relative to config dir
                let path = self.config_dir.join(pattern);
                results.push(path);
            }
        }

        Ok(results)
    }

    /// Resolve the output directory relative to the config file directory.
    pub fn resolve_output_dir(&self) -> PathBuf {
        self.config_dir.join(&self.config.output_dir)
    }
}

/// Check if a pattern contains glob characters.
fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_glob_pattern() {
        assert!(is_glob_pattern("*.png"));
        assert!(is_glob_pattern("sprites/*.png"));
        assert!(is_glob_pattern("sprites/**/*.png"));
        assert!(is_glob_pattern("sprite?.png"));
        assert!(is_glob_pattern("sprite[0-9].png"));
        assert!(!is_glob_pattern("sprite.png"));
        assert!(!is_glob_pattern("sprites/hero.png"));
    }
}
