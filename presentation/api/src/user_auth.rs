use crate::problem::UNAUTHENTICATED;
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

pub(crate) struct CurrentSession {
    pub(crate) token: String,
    pub(crate) user: domain::User,
}

pub(crate) struct UserAuthLayer;

impl Layer for UserAuthLayer {
    fn path(&self) -> Option<&Path> {
        Some(Path::new("/api"))
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
            let Some(user) =
                application::get_current_user::call(app_context::<AppState>(cx), token).await?
            else {
                return UNAUTHENTICATED.into_response(cx);
            };
            Span::current().set_attribute("user.id", user.id.to_string());
            let session = CurrentSession {
                token: token.to_owned(),
                user,
            };
            next.run(&cx.with(session), body).await
        })
    }
}
