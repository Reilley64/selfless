use application::invites::accept_invite::InviteNotFound;
use application::users::change_password::WrongCurrentPassword;
use application::users::create_user::NameTaken;
use ohno::ErrorExt;
use serde_json::json;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::router::Body;
use topcoat::router::Layer;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::Path;
use topcoat::router::StatusCode;
use topcoat::router::error::BadRequestError;
use topcoat::router::error::RewriteError;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::try_endpoint;

pub(crate) struct Problem {
    problem_type: &'static str,
    title: &'static str,
    status: StatusCode,
}

pub(crate) fn from_error<E>(cx: &Cx, error: E) -> topcoat::Result<Response>
where
    E: ErrorExt + Into<topcoat::Error>,
{
    if error.find_source::<WrongCurrentPassword>().is_some() {
        WRONG_CURRENT_PASSWORD.into_response(cx)
    } else if error.find_source::<InviteNotFound>().is_some() {
        INVITE_NOT_FOUND.into_response(cx)
    } else if error.find_source::<NameTaken>().is_some() {
        NAME_TAKEN.into_response(cx)
    } else {
        Err(error.into())
    }
}

const BAD_REQUEST: Problem = Problem {
    problem_type: "urn:selfless:problem:bad-request",
    title: "The request body is not valid.",
    status: StatusCode::BAD_REQUEST,
};

pub(crate) const INVALID_IDEMPOTENCY_KEY: Problem = Problem {
    problem_type: "urn:selfless:problem:invalid-idempotency-key",
    title: "The Idempotency-Key header is not valid.",
    status: StatusCode::BAD_REQUEST,
};

const INTERNAL_ERROR: Problem = Problem {
    problem_type: "urn:selfless:problem:internal-error",
    title: "Something went wrong on the server.",
    status: StatusCode::INTERNAL_SERVER_ERROR,
};

pub(crate) const WRONG_NAME_OR_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:wrong-name-or-password",
    title: "The name or password is wrong.",
    status: StatusCode::UNAUTHORIZED,
};

pub(crate) const UNAUTHENTICATED: Problem = Problem {
    problem_type: "urn:selfless:problem:unauthenticated",
    title: "The request has no valid session token.",
    status: StatusCode::UNAUTHORIZED,
};

const WRONG_CURRENT_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:wrong-current-password",
    title: "The current password is wrong.",
    status: StatusCode::FORBIDDEN,
};

pub(crate) const EMPTY_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:empty-password",
    title: "The password is empty.",
    status: StatusCode::UNPROCESSABLE_ENTITY,
};

const INVITE_NOT_FOUND: Problem = Problem {
    problem_type: "urn:selfless:problem:invite-not-found",
    title: "The invite link no longer works.",
    status: StatusCode::NOT_FOUND,
};

pub(crate) const NOT_FOUND: Problem = Problem {
    problem_type: "urn:selfless:problem:not-found",
    title: "Nothing exists at this URL.",
    status: StatusCode::NOT_FOUND,
};

pub(crate) const INVALID_SIGNATURE: Problem = Problem {
    problem_type: "urn:selfless:problem:invalid-signature",
    title: "The signed URL is not valid or has expired.",
    status: StatusCode::FORBIDDEN,
};

const NAME_TAKEN: Problem = Problem {
    problem_type: "urn:selfless:problem:name-taken",
    title: "A user already has this name.",
    status: StatusCode::CONFLICT,
};

impl IntoResponse for Problem {
    fn into_response(self, cx: &Cx) -> topcoat::Result<Response> {
        (
            self.status,
            [(CONTENT_TYPE, "application/problem+json")],
            json!({
                "type": self.problem_type,
                "title": self.title,
                "status": self.status.as_u16(),
            })
            .to_string(),
        )
            .into_response(cx)
    }
}

pub(crate) struct ProblemLayer;

impl Layer for ProblemLayer {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let error = match next.run(cx, body).await {
                Err(error) if !error.is::<RewriteError>() => error,
                result => return result,
            };
            let api = try_endpoint(cx).is_some_and(|x| x.path().starts_with(Path::new("/api")));
            if api && error.is::<BadRequestError>() {
                return BAD_REQUEST.into_response(cx);
            }
            if error.clone().into_response(cx)?.status() != StatusCode::INTERNAL_SERVER_ERROR {
                return Err(error);
            }
            tracing::error!(error = &*error as &dyn Error, "request failed");
            if api {
                INTERNAL_ERROR.into_response(cx)
            } else {
                Err(error)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ProblemLayer;
    use crate::test::serve;
    use crate::test::serve_web_app;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use http::Method;
    use http::StatusCode;
    use opentelemetry::logs::AnyValue;
    use opentelemetry::logs::Severity;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_sdk::logs::InMemoryLogExporter;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use serde_json::json;
    use test_app::TestApp;
    use test_fixtures::TestServer;
    use topcoat::router::Router;
    use topcoat::router::error::forbidden;
    use topcoat::router::error::see_other;
    use topcoat::router::response::Response;
    use topcoat::router::route;
    use tracing::level_filters::LevelFilter;
    use tracing::subscriber::DefaultGuard;
    use tracing_subscriber::layer::SubscriberExt;
    use uuid::Uuid;

    const NAME: &str = "Noor";
    const PASSWORD: &str = "a long passphrase about tea";

    #[route(GET "/failing")]
    async fn failing_page() -> topcoat::Result<Response> {
        Err(topcoat::Error::msg("the page failed"))
    }

    #[route(GET "/redirecting")]
    async fn redirecting_page() -> topcoat::Result<Response> {
        Err(see_other("/elsewhere").into())
    }

    #[route(GET "/forbidden")]
    async fn forbidden_page() -> topcoat::Result<Response> {
        Err(forbidden().into())
    }

    async fn serve_pages() -> TestServer {
        TestServer::serve(
            Router::builder()
                .route(failing_page)
                .route(redirecting_page)
                .route(forbidden_page)
                .layer(ProblemLayer)
                .build(),
        )
        .await
    }

    fn capture_logs(exporter: &InMemoryLogExporter) -> DefaultGuard {
        let provider = SdkLoggerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(OpenTelemetryTracingBridge::new(&provider)),
        )
    }

    fn errors(exporter: &InMemoryLogExporter) -> Vec<(Option<AnyValue>, String)> {
        exporter
            .get_emitted_logs()
            .unwrap()
            .iter()
            .filter(|x| x.record.severity_number() == Some(Severity::Error))
            .map(|x| {
                (
                    x.record.body().cloned(),
                    format!("{:?}", x.record.attributes_iter().collect::<Vec<_>>()),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn a_failing_use_case_behind_the_api_returns_the_internal_error_problem_and_logs_once() {
        let app = TestApp::new().await;
        signed_in_user(&app, NAME, PASSWORD).await;
        let mut db_context = app.state.db_context.clone();
        toasty::sql::statement("ALTER TABLE users RENAME TO users_gone")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;
        let exporter = InMemoryLogExporter::default();
        let _tracing = capture_logs(&exporter);

        let (status, problem) = server
            .request(
                Method::POST,
                "/api/sessions/v1",
                None,
                Some(json!({
                    "name": NAME,
                    "password": PASSWORD,
                    "clientKind": "web",
                    "deviceName": "Firefox on macOS",
                    "clientId": Uuid::new_v4(),
                })),
            )
            .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            problem,
            json!({
                "type": "urn:selfless:problem:internal-error",
                "title": "Something went wrong on the server.",
                "status": 500,
            })
        );
        let errors = errors(&exporter);
        assert_eq!(errors.len(), 1, "{errors:?}");
        let (message, attributes) = &errors[0];
        assert_eq!(message, &Some(AnyValue::from("request failed")));
        assert!(attributes.contains("users"), "{attributes}");
        assert!(!attributes.contains(NAME), "{attributes}");
        assert!(!attributes.contains(PASSWORD), "{attributes}");
    }

    #[tokio::test]
    async fn a_failing_page_outside_the_api_returns_a_500_and_logs_once() {
        let server = serve_pages().await;
        let exporter = InMemoryLogExporter::default();
        let _tracing = capture_logs(&exporter);

        let page = server.browser().get("/failing").await;

        assert_eq!(page.status, StatusCode::INTERNAL_SERVER_ERROR);
        let errors = errors(&exporter);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].0, Some(AnyValue::from("request failed")));
    }

    #[tokio::test]
    async fn errors_that_are_not_server_failures_and_web_app_pages_log_nothing() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;
        let pages = serve_pages().await;
        let exporter = InMemoryLogExporter::default();
        let _tracing = capture_logs(&exporter);
        let mut browser = pages.browser();

        let (bad_request, problem) = server
            .request(Method::POST, "/api/sessions/v1", None, Some(json!({})))
            .await;
        let (not_found, _, _) = server.get_text("/api/nothing/v1").await;
        let (settings_page, _, _) = server.get_text("/settings").await;
        let redirect = browser.get("/redirecting").await;
        let forbidden = browser.get("/forbidden").await;

        assert_eq!(bad_request, StatusCode::BAD_REQUEST);
        assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
        assert_eq!(not_found, StatusCode::NOT_FOUND);
        assert_eq!(settings_page, StatusCode::OK);
        assert_eq!(redirect.status, StatusCode::SEE_OTHER);
        assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
        assert_eq!(errors(&exporter), []);
        std::fs::remove_dir_all(web_dir).unwrap();
    }
}
