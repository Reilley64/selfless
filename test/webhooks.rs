use application::webhooks::WebhookPasswords;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use http::HeaderMap;
use http::Method;
use http::StatusCode;
use http::header::AUTHORIZATION;
use serde_json::json;
use test_app::TestApp;
use test_fixtures::TestServer;
use topcoat::router::Router;

pub(crate) const RADARR_PASSWORD: &str = "radarr webhook password";
pub(crate) const SONARR_PASSWORD: &str = "sonarr webhook password";

pub(crate) async fn serve(app: &TestApp) -> TestServer {
    let mut state = app.state.clone();
    state.webhook_passwords = WebhookPasswords {
        radarr: RADARR_PASSWORD.to_owned(),
        sonarr: SONARR_PASSWORD.to_owned(),
    };
    TestServer::serve(
        crate::register(Router::builder())
            .app_context(state)
            .build(),
    )
    .await
}

pub(crate) async fn send(
    server: &TestServer,
    method: Method,
    path: &str,
    credentials: Option<(&str, &str)>,
) -> (StatusCode, HeaderMap) {
    let mut headers = HeaderMap::new();
    if let Some((username, password)) = credentials {
        let basic = STANDARD.encode(format!("{username}:{password}"));
        headers.insert(AUTHORIZATION, format!("Basic {basic}").parse().unwrap());
    }
    let (status, headers, _) = server
        .request_with_response_headers(method, path, headers, Some(json!({"eventType": "Test"})))
        .await;
    (status, headers)
}
