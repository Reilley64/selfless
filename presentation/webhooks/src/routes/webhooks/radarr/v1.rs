use crate::routes::webhooks::delivery;
use domain::Source;
use topcoat::context::Cx;
use topcoat::router::request::Bytes;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route([POST, PUT] "/webhooks/radarr/v1")]
pub(crate) async fn radarr(cx: &Cx, body: Bytes) -> topcoat::Result<Response> {
    delivery::store(cx, Source::Radarr, body).await
}

#[cfg(test)]
mod tests {
    use crate::test::RADARR_PASSWORD;
    use crate::test::SONARR_PASSWORD;
    use crate::test::send;
    use crate::test::serve;
    use http::Method;
    use http::StatusCode;
    use test_app::TestApp;

    const PATH: &str = "/webhooks/radarr/v1";

    #[tokio::test]
    async fn post_and_put_accept_the_right_password() {
        let server = serve(&TestApp::new().await).await;

        for method in [Method::POST, Method::PUT] {
            let (status, _) = send(
                &server,
                method.clone(),
                PATH,
                Some(("radarr", RADARR_PASSWORD)),
            )
            .await;

            assert_eq!(status, StatusCode::OK, "{method}");
        }
    }

    #[tokio::test]
    async fn the_sonarr_password_is_rejected() {
        let server = serve(&TestApp::new().await).await;

        let (status, _) = send(
            &server,
            Method::POST,
            PATH,
            Some(("radarr", SONARR_PASSWORD)),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
