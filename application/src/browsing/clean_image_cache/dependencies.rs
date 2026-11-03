use crate::ports::DeleteCachedImage;
use crate::ports::ListCachedImages;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub list_cached_images: ListCachedImages,
    pub delete_cached_image: DeleteCachedImage,
}
