use crate::problem::UNAUTHENTICATED;
use crate::routes::api::user::UserBody;
use application::sessions::get_current_user;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::Body;
use topcoat::router::Layer;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::Path;
use topcoat::router::header::AUTHORIZATION;
use topcoat::router::request::headers;
use topcoat::router::response::IntoResponse;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

pub(crate) struct CurrentSession {
    pub(crate) id: Uuid,
    pub(crate) token: String,
    pub(crate) user: UserBody,
}

pub(crate) struct UserAuthLayer;

impl Layer for UserAuthLayer {
    fn path(&self) -> Option<&Path> {
        Some(Path::new("/api/(user)"))
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let Some(token) = headers(cx)
                .get(AUTHORIZATION)
                .and_then(|x| x.to_str().ok())
                .and_then(|x| x.strip_prefix("Bearer "))
            else {
                return UNAUTHENTICATED.into_response(cx);
            };
            let get_current_user::Output::SignedIn {
                id,
                session_id,
                name,
                colour,
            } = get_current_user::call(app_context::<AppState>(cx), token).await?
            else {
                return UNAUTHENTICATED.into_response(cx);
            };
            Span::current().set_attribute("user.id", id.to_string());
            let session = CurrentSession {
                id: session_id,
                token: token.to_owned(),
                user: UserBody {
                    id,
                    name,
                    colour: colour.into(),
                },
            };
            next.run(&cx.with(session), body).await
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::test::serve;
    use crate::test::serve_web_app;
    use crate::test::signed_in_user;
    use application::admins::create_admin;
    use application::sessions::admin_sign_in;
    use domain::Password;
    use http::HeaderMap;
    use http::Method;
    use http::StatusCode;
    use http::header::COOKIE;
    use jiff::SignedDuration;
    use opentelemetry::KeyValue;
    use opentelemetry::trace::SpanKind;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use test_app::TestApp;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const NAME: &str = "Noor";
    const PASSWORD: &str = "a long passphrase about tea";
    const UNAUTHENTICATED: &str = "urn:selfless:problem:unauthenticated";

    #[tokio::test]
    async fn a_missing_or_unknown_token_returns_401() {
        let app = TestApp::new().await;
        let server = serve(&app).await;

        let missing = server.request(Method::GET, "/api/me/v1", None, None).await;
        let unknown = server
            .request(
                Method::GET,
                "/api/me/v1",
                Some("q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M"),
                None,
            )
            .await;

        assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
        assert_eq!(missing.1["type"], UNAUTHENTICATED);
        assert_eq!(missing, unknown);
    }

    #[tokio::test]
    async fn a_web_session_ends_after_30_days_without_use() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;
        app.clock.advance(SignedDuration::from_hours(30 * 24));

        let (status, problem) = server
            .request(Method::GET, "/api/me/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(problem["type"], UNAUTHENTICATED);
    }

    #[tokio::test]
    async fn an_admin_session_is_rejected_on_the_api() {
        let app = TestApp::new().await;
        create_admin::call(
            &app.state,
            "Ines".to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let admin_sign_in::Output::SignedIn { token } =
            admin_sign_in::call(&app.state, "Ines".to_owned(), PASSWORD.to_owned())
                .await
                .unwrap()
        else {
            panic!("wrong name or password");
        };
        let server = serve(&app).await;
        let mut cookie = HeaderMap::new();
        cookie.insert(COOKIE, format!("session={token}").parse().unwrap());

        let with_bearer = server
            .request(Method::GET, "/api/me/v1", Some(&token), None)
            .await;
        let with_cookie = server
            .request_with_headers(Method::GET, "/api/me/v1", None, cookie, None)
            .await;

        assert_eq!(with_bearer.0, StatusCode::UNAUTHORIZED);
        assert_eq!(with_bearer.1["type"], UNAUTHENTICATED);
        assert_eq!(with_cookie, with_bearer);
    }

    #[tokio::test]
    async fn the_actuator_endpoints_and_the_web_app_stay_public() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;

        let (info, _) = server.get("/actuator/info/v1").await;
        let (health, _) = server.get("/actuator/health/v1").await;
        let (index, _, html) = server.get_text("/").await;
        let (script, _, _) = server.get_text("/app.js").await;

        assert_eq!(info, StatusCode::OK);
        assert_eq!(health, StatusCode::OK);
        assert_eq!(index, StatusCode::OK);
        assert!(html.contains("<title>selfless</title>"), "{html}");
        assert_eq!(script, StatusCode::OK);
        std::fs::remove_dir_all(web_dir).unwrap();
    }

    #[tokio::test]
    async fn authenticated_request_spans_carry_the_user_id() {
        let app = TestApp::new().await;
        let (user_id, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );

        server
            .request(Method::GET, "/api/me/v1", Some(&token), None)
            .await;

        let spans = exporter.get_finished_spans().unwrap();
        let http = spans
            .iter()
            .find(|x| x.span_kind == SpanKind::Server)
            .expect("an HTTP server span");
        assert_eq!(http.name, "GET /api/me/v1");
        assert!(
            http.attributes
                .contains(&KeyValue::new("user.id", user_id.to_string())),
            "{:?}",
            http.attributes
        );
    }
}
