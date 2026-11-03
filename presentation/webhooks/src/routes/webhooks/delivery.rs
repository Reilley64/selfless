use application::webhooks::store_webhook_delivery;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::Source;
use infrastructure::AppState;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::header::AUTHORIZATION;
use topcoat::router::header::WWW_AUTHENTICATE;
use topcoat::router::request::Bytes;
use topcoat::router::request::headers;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;

pub(crate) async fn store(cx: &Cx, source: Source, body: Bytes) -> topcoat::Result<Response> {
    let password = headers(cx)
        .get(AUTHORIZATION)
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Basic "))
        .and_then(|x| STANDARD.decode(x).ok())
        .and_then(|x| String::from_utf8(x).ok())
        .and_then(|x| x.split_once(':').map(|(_, password)| password.to_owned()));
    match store_webhook_delivery::call(app_context::<AppState>(cx), source, password, body).await {
        Ok(store_webhook_delivery::Output::Stored) => StatusCode::OK.into_response(cx),
        Ok(store_webhook_delivery::Output::Rejected) => (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, r#"Basic realm="selfless""#)],
        )
            .into_response(cx),
        Err(error) => {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to store a webhook delivery"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response(cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test::RADARR_PASSWORD;
    use crate::test::send;
    use crate::test::serve;
    use http::Method;
    use http::StatusCode;
    use http::header::WWW_AUTHENTICATE;
    use test_app::TestApp;

    const PATH: &str = "/webhooks/radarr/v1";

    #[tokio::test]
    async fn a_request_without_an_authorization_header_gets_a_basic_challenge() {
        let server = serve(&TestApp::new().await).await;

        let (status, headers) = send(&server, Method::POST, PATH, None).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers[WWW_AUTHENTICATE], r#"Basic realm="selfless""#);
    }

    #[tokio::test]
    async fn the_right_password_is_accepted_with_an_empty_or_any_username() {
        let server = serve(&TestApp::new().await).await;

        for username in ["", "anyone"] {
            let (status, _) = send(
                &server,
                Method::POST,
                PATH,
                Some((username, RADARR_PASSWORD)),
            )
            .await;

            assert_eq!(status, StatusCode::OK, "{username:?}");
        }
    }
}
