use application::attributes::SELFLESS_IMAGES_WIDTH;
use application::ports::ImageFile;
use application::ports::ImageFormat;
use application::ports::ResizeImage;
use application::ports::ResizeImageError;
use application::spans::SELFLESS_IMAGES_RESIZE;
use image::DynamicImage;
use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use std::io::Cursor;

const JPEG_QUALITY: u8 = 80;

pub fn resize_image() -> ResizeImage {
    ResizeImage::new(|path, width| {
        let span = tracing::info_span!(
            "image_resize",
            otel.name = SELFLESS_IMAGES_RESIZE,
            otel.kind = "internal",
            { SELFLESS_IMAGES_WIDTH } = width,
        );
        Box::pin(async move {
            tokio::task::spawn_blocking(move || span.in_scope(|| resize(&path, width)))
                .await
                .map_err(ResizeImageError::caused_by)?
        })
    })
}

fn resize(path: &str, width: i64) -> Result<ImageFile, ResizeImageError> {
    let source = std::fs::read(path).map_err(ResizeImageError::caused_by)?;
    let reader = ImageReader::new(Cursor::new(&source))
        .with_guessed_format()
        .map_err(ResizeImageError::caused_by)?;
    let format = match reader.format() {
        Some(image::ImageFormat::Jpeg) => ImageFormat::Jpeg,
        Some(image::ImageFormat::Png) => ImageFormat::Png,
        _ => {
            return Err(ResizeImageError::caused_by(
                "the image is not a JPEG or PNG",
            ));
        }
    };
    let image = reader.decode().map_err(ResizeImageError::caused_by)?;
    let width = u32::try_from(width).map_err(ResizeImageError::caused_by)?;
    if image.width() <= width {
        return Ok(ImageFile {
            format,
            bytes: source.into(),
        });
    }
    let height = (u64::from(image.height()) * u64::from(width) / u64::from(image.width())).max(1);
    let image = image.resize_exact(
        width,
        u32::try_from(height).map_err(ResizeImageError::caused_by)?,
        FilterType::Lanczos3,
    );
    let mut bytes = Vec::new();
    match format {
        ImageFormat::Jpeg => DynamicImage::ImageRgb8(image.to_rgb8())
            .write_with_encoder(JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)),
        ImageFormat::Png => image.write_with_encoder(PngEncoder::new(&mut bytes)),
    }
    .map_err(ResizeImageError::caused_by)?;
    Ok(ImageFile {
        format,
        bytes: bytes.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;

    fn fixture(name: &str) -> String {
        format!(
            "{}/../../test/fixtures/images/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    async fn resized(name: &str, width: i64) -> (ImageFormat, image::DynamicImage) {
        let file = resize_image().call(&fixture(name), width).await.unwrap();
        (file.format, image::load_from_memory(&file.bytes).unwrap())
    }

    #[tokio::test]
    async fn a_jpeg_is_resized_to_the_width_as_a_jpeg_keeping_its_aspect_ratio() {
        let (format, image) = resized("fanart.jpg", 480).await;

        assert_eq!(format, ImageFormat::Jpeg);
        assert_eq!(image.dimensions(), (480, 270));
    }

    #[tokio::test]
    async fn a_png_logo_stays_a_png_with_its_transparency() {
        let (format, image) = resized("clearlogo.png", 300).await;

        assert_eq!(format, ImageFormat::Png);
        assert_eq!(image.dimensions(), (300, 116));
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        assert_eq!(image.get_pixel(150, 58).0[3], 255);
    }

    #[tokio::test]
    async fn an_image_smaller_than_the_width_is_served_at_its_own_size() {
        let file = resize_image()
            .call(&fixture("small-thumb.jpg"), 480)
            .await
            .unwrap();

        assert_eq!(file.format, ImageFormat::Jpeg);
        assert_eq!(
            file.bytes,
            std::fs::read(fixture("small-thumb.jpg")).unwrap()
        );
    }

    #[tokio::test]
    async fn each_resize_is_a_span_with_the_width() {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );

        resize_image()
            .call(&fixture("fanart.jpg"), 480)
            .await
            .unwrap();

        let spans = exporter.get_finished_spans().unwrap();
        let [span] = &spans[..] else {
            panic!("one span: {spans:?}");
        };
        assert_eq!(span.name, "selfless.images.resize");
        assert!(
            span.attributes
                .contains(&KeyValue::new("selfless.images.width", 480)),
            "{:?}",
            span.attributes
        );
    }
}
