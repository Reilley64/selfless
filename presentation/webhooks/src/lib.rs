use application::store_webhook_delivery::Delivery;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::Source;
use infrastructure::AppState;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::RouterBuilder;
use topcoat::router::StatusCode;
use topcoat::router::header::AUTHORIZATION;
use topcoat::router::header::WWW_AUTHENTICATE;
use topcoat::router::request::Bytes;
use topcoat::router::request::headers;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router.route(radarr)
}

#[route([POST, PUT] "/webhooks/radarr/v1")]
async fn radarr(cx: &Cx, body: Bytes) -> topcoat::Result<Response> {
    store(cx, Source::Radarr, body).await
}

async fn store(cx: &Cx, source: Source, body: Bytes) -> topcoat::Result<Response> {
    let password = headers(cx)
        .get(AUTHORIZATION)
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Basic "))
        .and_then(|x| STANDARD.decode(x).ok())
        .and_then(|x| String::from_utf8(x).ok())
        .and_then(|x| x.split_once(':').map(|(_, password)| password.to_owned()));
    match application::store_webhook_delivery::call(
        app_context::<AppState>(cx),
        source,
        password,
        body,
    )
    .await
    {
        Ok(Delivery::Stored) => StatusCode::OK.into_response(cx),
        Ok(Delivery::Rejected) => (
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
