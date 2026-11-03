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
    async fn post_and_put_accept_the_right_password_with_any_username() {
        let server = serve(&TestApp::new().await).await;

        for (method, username) in [(Method::POST, "sonarr"), (Method::PUT, "anyone")] {
            let (status, _) = send(&server, method.clone(), PATH, (username, PASSWORD)).await;

            assert_eq!(status, StatusCode::OK, "{method} {username:?}");
        }
    }

    #[tokio::test]
    async fn a_wrong_password_gets_a_basic_challenge() {
        let server = serve(&TestApp::new().await).await;

        let (status, headers) =
            send(&server, Method::POST, PATH, ("sonarr", "wrong password")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers[WWW_AUTHENTICATE], r#"Basic realm="selfless""#);
    }
}
