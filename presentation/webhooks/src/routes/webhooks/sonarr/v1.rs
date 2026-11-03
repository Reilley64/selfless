use crate::routes::webhooks::delivery;
use domain::Source;
use topcoat::context::Cx;
use topcoat::router::request::Bytes;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route([POST, PUT] "/webhooks/sonarr/v1")]
pub(crate) async fn sonarr(cx: &Cx, body: Bytes) -> topcoat::Result<Response> {
    delivery::store(cx, Source::Sonarr, body).await
}

#[cfg(test)]
mod tests {
    use crate::test::PASSWORD;
    use crate::test::send;
    use crate::test::serve;
    use http::Method;
    use http::StatusCode;
    use http::header::WWW_AUTHENTICATE;
    use test_app::TestApp;

    const PATH: &str = "/webhooks/sonarr/v1";

    #[tokio::test]
    async fn the_right_password_is_accepted() {
        let server = serve(&TestApp::new().await).await;

        let (status, _) = send(&server, Method::POST, PATH, Some(("sonarr", PASSWORD))).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn a_wrong_or_missing_password_gets_a_basic_challenge() {
        let server = serve(&TestApp::new().await).await;

        for credentials in [Some(("sonarr", "wrong password")), None] {
            let (status, headers) = send(&server, Method::POST, PATH, credentials).await;

            assert_eq!(status, StatusCode::UNAUTHORIZED, "{credentials:?}");
            assert_eq!(headers[WWW_AUTHENTICATE], r#"Basic realm="selfless""#);
        }
    }
}
