mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_IMAGES_CACHE;
use crate::attributes::selfless_images_cache;
use crate::browsing::image::resource;
use crate::metrics::SELFLESS_IMAGES_REQUESTS;
use crate::ports::CachedImage;
use crate::ports::persistence::DbScope;
use jiff::Timestamp;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Counter;
use std::sync::LazyLock;
use uuid::Uuid;

static REQUESTS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    global::meter("selfless")
        .u64_counter(SELFLESS_IMAGES_REQUESTS)
        .with_unit("{request}")
        .build()
});

pub async fn call(
    dependencies: impl Into<Dependencies>,
    image_id: Uuid,
    width: i64,
    expires: Timestamp,
    signature: &str,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
        read_cached_image,
        resizer,
        save_cached_image,
    } = dependencies.into();
    let Some(artwork) = db_context
        .artwork()
        .find(image_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    if !artwork.kind.widths().contains(&width) {
        return Ok(Output::NotFound);
    }
    let now = clock.call();
    if !url_signer.verify(
        &resource(artwork.id, width, artwork.modified_at),
        expires,
        signature,
        now,
    ) {
        return Ok(Output::InvalidSignature);
    }
    let cached = CachedImage {
        image_id,
        width,
        modified_at: artwork.modified_at,
    };
    let (file, cache) = match read_cached_image
        .call(&cached)
        .await
        .map_err(Error::caused_by)?
    {
        Some(file) => (file, selfless_images_cache::HIT),
        None => {
            let file = resizer
                .resize(&artwork.path, width)
                .await
                .map_err(Error::caused_by)?;
            save_cached_image
                .call(&cached, file.clone())
                .await
                .map_err(Error::caused_by)?;
            (file, selfless_images_cache::RESIZED)
        }
    };
    REQUESTS.add(1, &[KeyValue::new(SELFLESS_IMAGES_CACHE, cache)]);
    Ok(Output::Image {
        file,
        max_age: expires.duration_since(now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::ImageFile;
    use crate::ports::ImageFormat;
    use crate::ports::persistence::DbContext;
    use crate::test::FakeImageCache;
    use crate::test::FakeResizer;
    use crate::test::mock_db_context;
    use crate::test::url_signer;
    use crate::url_signer::SignedUrl;
    use domain::Artwork;
    use domain::ArtworkKind;
    use jiff::SignedDuration;
    use test_fixtures::FakeClock;

    const FANART: &str = "/mnt/media/movies/Arrival (2016)/fanart.jpg";

    struct Fixture {
        db_context: DbContext,
        clock: FakeClock,
        cache: FakeImageCache,
        resizer: FakeResizer,
    }

    impl Fixture {
        async fn new() -> Self {
            let clock = FakeClock::new();
            clock.set("2026-11-03T10:00:00Z".parse().unwrap());
            Self {
                db_context: mock_db_context().await,
                clock,
                cache: FakeImageCache::default(),
                resizer: FakeResizer::new(),
            }
        }

        async fn add_artwork(&self, kind: ArtworkKind, modified_at: &str) -> Artwork {
            let mut db_context = self.db_context.clone();
            db_context
                .artwork()
                .create()
                .kind(kind)
                .path(FANART)
                .modified_at(modified_at.parse::<Timestamp>().unwrap())
                .exec(db_context.executor_mut())
                .await
                .unwrap()
        }

        fn sign(&self, artwork: &Artwork, width: i64) -> SignedUrl {
            url_signer().sign(
                &resource(artwork.id, width, artwork.modified_at),
                self.clock.now(),
            )
        }

        async fn get(&self, image_id: Uuid, width: i64, url: &SignedUrl) -> Output {
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                    url_signer: url_signer(),
                    read_cached_image: self.cache.read(),
                    resizer: self.resizer.resizer(),
                    save_cached_image: self.cache.save(),
                },
                image_id,
                width,
                url.expires,
                &url.signature,
            )
            .await
            .unwrap()
        }
    }

    fn served(output: Output) -> String {
        match output {
            Output::Image { file, .. } => {
                assert_eq!(file.format, ImageFormat::Jpeg);
                String::from_utf8(file.bytes.to_vec()).unwrap()
            }
            output => panic!("not an image: {output:?}"),
        }
    }

    #[tokio::test]
    async fn the_first_request_for_each_width_resizes_the_source() {
        let fixture = Fixture::new().await;
        let fanart = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;

        let mut served_widths = Vec::new();
        for width in [480, 960, 1280, 1920] {
            served_widths.push(served(
                fixture
                    .get(fanart.id, width, &fixture.sign(&fanart, width))
                    .await,
            ));
        }

        assert_eq!(
            served_widths,
            [480, 960, 1280, 1920].map(|x| format!("{FANART} at {x}"))
        );
        assert_eq!(
            fixture.resizer.resized(),
            [480, 960, 1280, 1920].map(|x| (FANART.to_owned(), x))
        );
    }

    #[tokio::test]
    async fn a_width_the_image_does_not_come_in_or_an_unknown_image_is_not_found() {
        let fixture = Fixture::new().await;
        let logo = fixture
            .add_artwork(ArtworkKind::Clearlogo, "2026-10-01T08:00:00Z")
            .await;
        let poster = fixture
            .add_artwork(ArtworkKind::Poster, "2026-10-01T08:00:00Z")
            .await;

        let wrong_width = fixture.get(logo.id, 480, &fixture.sign(&logo, 480)).await;
        let poster = fixture
            .get(poster.id, 480, &fixture.sign(&poster, 480))
            .await;
        let unknown = fixture
            .get(Uuid::new_v4(), 300, &fixture.sign(&logo, 300))
            .await;

        assert!(matches!(wrong_width, Output::NotFound), "{wrong_width:?}");
        assert!(matches!(poster, Output::NotFound), "{poster:?}");
        assert!(matches!(unknown, Output::NotFound), "{unknown:?}");
        assert!(fixture.resizer.resized().is_empty());
    }

    #[tokio::test]
    async fn a_cached_image_is_served_without_resizing_it_again() {
        let fixture = Fixture::new().await;
        let fanart = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;
        let url = fixture.sign(&fanart, 480);

        let first = served(fixture.get(fanart.id, 480, &url).await);
        let second = served(fixture.get(fanart.id, 480, &url).await);

        assert_eq!(first, second);
        assert_eq!(fixture.resizer.resized().len(), 1);
    }

    #[tokio::test]
    async fn a_changed_image_gets_a_new_url_and_a_new_resize() {
        let fixture = Fixture::new().await;
        let old = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;
        let old_url = fixture.sign(&old, 480);
        fixture.get(old.id, 480, &old_url).await;
        let mut db_context = fixture.db_context.clone();
        db_context
            .artwork()
            .filter(Artwork::fields().id().eq(old.id))
            .delete()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let new = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-11-02T21:30:00Z")
            .await;

        let new_url = fixture.sign(&new, 480);
        let served_new = fixture.get(new.id, 480, &new_url).await;
        let served_old = fixture.get(old.id, 480, &old_url).await;

        assert_ne!((new.id, &new_url.signature), (old.id, &old_url.signature));
        served(served_new);
        assert!(matches!(served_old, Output::NotFound), "{served_old:?}");
        assert_eq!(fixture.resizer.resized().len(), 2);
    }

    #[tokio::test]
    async fn an_image_whose_modification_time_changed_refuses_its_old_signature() {
        let fixture = Fixture::new().await;
        let mut fanart = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;
        let old_url = fixture.sign(&fanart, 480);
        let mut db_context = fixture.db_context.clone();
        fanart
            .update()
            .modified_at("2026-11-02T21:30:00Z".parse::<Timestamp>().unwrap())
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let output = fixture.get(fanart.id, 480, &old_url).await;

        assert!(matches!(output, Output::InvalidSignature), "{output:?}");
    }

    #[tokio::test]
    async fn an_expired_or_tampered_signature_is_refused_without_resizing() {
        let fixture = Fixture::new().await;
        let fanart = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;
        let url = fixture.sign(&fanart, 480);
        let tampered = SignedUrl {
            expires: url.expires,
            signature: url.signature.chars().rev().collect(),
        };
        let other_width = fixture.sign(&fanart, 960);

        let tampered = fixture.get(fanart.id, 480, &tampered).await;
        let other_width = fixture.get(fanart.id, 480, &other_width).await;
        fixture.clock.set(url.expires);
        let expired = fixture.get(fanart.id, 480, &url).await;

        assert!(matches!(tampered, Output::InvalidSignature), "{tampered:?}");
        assert!(
            matches!(other_width, Output::InvalidSignature),
            "{other_width:?}"
        );
        assert!(matches!(expired, Output::InvalidSignature), "{expired:?}");
        assert!(fixture.resizer.resized().is_empty());
    }

    #[tokio::test]
    async fn an_image_may_be_cached_until_its_url_expires() {
        let fixture = Fixture::new().await;
        let fanart = fixture
            .add_artwork(ArtworkKind::Fanart, "2026-10-01T08:00:00Z")
            .await;
        let url = fixture.sign(&fanart, 480);
        fixture.clock.set("2026-11-04T12:00:00Z".parse().unwrap());

        let output = fixture.get(fanart.id, 480, &url).await;

        let Output::Image { file, max_age } = output else {
            panic!("not an image: {output:?}");
        };
        assert_eq!(max_age, SignedDuration::from_hours(12));
        assert_eq!(
            file,
            ImageFile {
                format: ImageFormat::Jpeg,
                bytes: format!("{FANART} at 480").into(),
            }
        );
    }
}
