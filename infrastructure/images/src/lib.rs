mod cache;
mod resize;

pub use cache::delete_cached_image;
pub use cache::list_cached_images;
pub use cache::read_cached_image;
pub use cache::save_cached_image;
pub use resize::resize_image;

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Default, Deserialize)]
pub struct ImagesSettings {
    pub cache_dir: Option<PathBuf>,
}

impl ImagesSettings {
    pub fn cache_dir(&self) -> Result<PathBuf, MissingSetting> {
        self.cache_dir
            .clone()
            .ok_or_else(|| MissingSetting::new("IMAGES__CACHE_DIR"))
    }
}

#[ohno::error]
#[display("missing setting: {name}")]
pub struct MissingSetting {
    name: &'static str,
}
