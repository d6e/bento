mod loader;
mod trimmer;
mod types;

pub use loader::load_sprites;
pub use trimmer::trim_sprite;
pub use types::{PackedSprite, SourceSprite, TrimInfo};
