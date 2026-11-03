use crate::problem::INVALID_SIGNATURE;
use crate::problem::NOT_FOUND;
use application::browsing::get_image::Output;
use application::ports::ImageFormat;
use infrastructure::AppState;
use jiff::Timestamp;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::header::CACHE_CONTROL;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::path_param;
use topcoat::router::query_params;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

path_param!(image_id: Uuid);
path_param!(width: i64);

#[query_params]
struct Signed {
    expires: Option<i64>,
    signature: Option<String>,
}

#[route(GET "/api/(public)/images/{image_id}/{width}/v1")]
pub(crate) async fn image(cx: &Cx) -> topcoat::Result<Response> {
    let (Ok(image_id), Ok(width)) = (path_param::<ImageId>(cx), path_param::<Width>(cx)) else {
        return NOT_FOUND.into_response(cx);
    };
    let Some((expires, signature)) = query_params::<Signed>(cx).ok().and_then(|x| {
        Some((
            Timestamp::from_second(x.expires?).ok()?,
            x.signature.as_deref()?,
        ))
    }) else {
        return INVALID_SIGNATURE.into_response(cx);
    };
    match application::browsing::get_image::call(
        app_context::<AppState>(cx),
        *image_id,
        *width,
        expires,
        signature,
    )
    .await?
    {
        Output::Image { file, max_age } => (
            [
                (
                    CONTENT_TYPE,
                    match file.format {
                        ImageFormat::Jpeg => "image/jpeg".to_owned(),
                        ImageFormat::Png => "image/png".to_owned(),
                    },
                ),
                (
                    CACHE_CONTROL,
                    format!("private, max-age={}", max_age.as_secs()),
                ),
            ],
            file.bytes,
        )
            .into_response(cx),
        Output::NotFound => NOT_FOUND.into_response(cx),
        Output::InvalidSignature => INVALID_SIGNATURE.into_response(cx),
    }
}

#[cfg(test)]
mod tests {
    use crate::test::add_artwork;
    use crate::test::add_movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::browsing::Resizer;
    use application::ports::ImageFile;
    use application::ports::ImageFormat;
    use application::ports::ResizeImage;
    use domain::ArtworkKind;
    use http::Method;
    use http::StatusCode;
    use http::header::CACHE_CONTROL;
    use http::header::CONTENT_TYPE;
    use serde_json::Value;
    use test_app::TestApp;
    use test_fixtures::TestServer;
    use uuid::Uuid;

    async fn app() -> TestApp {
        let mut app = TestApp::new().await;
        app.clock.set("2026-11-03T10:00:00Z".parse().unwrap());
        app.state.resizer = Resizer::new(ResizeImage::new(|path, width| {
            Box::pin(async move {
                Ok(ImageFile {
                    format: if path.ends_with(".png") {
                        ImageFormat::Png
                    } else {
                        ImageFormat::Jpeg
                    },
                    bytes: format!("{path} at {width}").into(),
                })
            })
        }));
        app
    }

    async fn tile_images(app: &TestApp, server: &TestServer) -> Value {
        let (_, token) = signed_in_user(app, "Noor", "a long passphrase about tea").await;
        let (_, page) = server
            .request(Method::GET, "/api/movies/v1", Some(&token), None)
            .await;
        page["tiles"][0]["images"].clone()
    }

    fn url(image: &Value, width: i64) -> String {
        image["urls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["width"] == width)
            .unwrap_or_else(|| panic!("no URL at {width}: {image}"))["url"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn a_tile_image_url_serves_the_image_without_an_authorization_header() {
        let app = app().await;
        let movie_id = add_movie(&app, "Arrival").await;
        add_artwork(
            &app,
            movie_id,
            ArtworkKind::Fanart,
            "/movies/Arrival/fanart.jpg",
        )
        .await;
        add_artwork(
            &app,
            movie_id,
            ArtworkKind::Clearlogo,
            "/movies/Arrival/clearlogo.png",
        )
        .await;
        let server = serve(&app).await;
        let images = tile_images(&app, &server).await;

        let (fanart_status, fanart_headers, fanart) =
            server.get_bytes(&url(&images["fanart"], 480)).await;
        let (logo_status, logo_headers, logo) = server.get_bytes(&url(&images["logo"], 600)).await;

        assert_eq!(fanart_status, StatusCode::OK);
        assert_eq!(fanart_headers[CONTENT_TYPE], "image/jpeg");
        assert_eq!(fanart_headers[CACHE_CONTROL], "private, max-age=136800");
        assert_eq!(fanart, b"/movies/Arrival/fanart.jpg at 480");
        assert_eq!(logo_status, StatusCode::OK);
        assert_eq!(logo_headers[CONTENT_TYPE], "image/png");
        assert_eq!(logo, b"/movies/Arrival/clearlogo.png at 600");
    }

    #[tokio::test]
    async fn a_bad_or_missing_signature_returns_the_invalid_signature_problem() {
        let app = app().await;
        let movie_id = add_movie(&app, "Arrival").await;
        add_artwork(
            &app,
            movie_id,
            ArtworkKind::Fanart,
            "/movies/Arrival/fanart.jpg",
        )
        .await;
        let server = serve(&app).await;
        let url = url(&tile_images(&app, &server).await["fanart"], 480);
        let (path, query) = url.split_once('?').unwrap();

        for url in [
            format!("{url}x"),
            path.to_owned(),
            format!("{path}?{}", query.replace("expires=", "expires=x")),
        ] {
            let (status, _, problem) = server.get_bytes(&url).await;

            assert_eq!(status, StatusCode::FORBIDDEN, "{url}");
            let problem = serde_json::from_slice::<Value>(&problem).unwrap();
            assert_eq!(problem["type"], "urn:selfless:problem:invalid-signature");
        }
    }

    #[tokio::test]
    async fn an_unknown_image_or_width_returns_the_not_found_problem() {
        let app = app().await;
        let movie_id = add_movie(&app, "Arrival").await;
        let fanart = add_artwork(
            &app,
            movie_id,
            ArtworkKind::Fanart,
            "/movies/Arrival/fanart.jpg",
        )
        .await;
        let server = serve(&app).await;
        let url = url(&tile_images(&app, &server).await["fanart"], 480);
        let query = url.split_once('?').unwrap().1;

        for url in [
            url.replace("/480/", "/300/"),
            url.replace("/480/", "/wide/"),
            url.replace(&fanart.to_string(), &Uuid::new_v4().to_string()),
            format!("/api/images/fanart/480/v1?{query}"),
        ] {
            let (status, _, problem) = server.get_bytes(&url).await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{url}");
            let problem = serde_json::from_slice::<Value>(&problem).unwrap();
            assert_eq!(problem["type"], "urn:selfless:problem:not-found");
        }
    }
}
