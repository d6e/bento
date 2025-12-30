mod loader;
mod sprite;
mod trimmer;

pub use loader::load_sprites;
pub use sprite::{PackedSprite, SourceSprite, TrimInfo};
pub use trimmer::trim_sprite;
