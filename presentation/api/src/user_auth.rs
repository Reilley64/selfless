use crate::problem::UNAUTHENTICATED;
use crate::routes::api::me::v1::UserBody;
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

pub(crate) struct CurrentSession {
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
                name,
                colour,
                symbol,
            } = get_current_user::call(app_context::<AppState>(cx), token).await?
            else {
                return UNAUTHENTICATED.into_response(cx);
            };
            Span::current().set_attribute("user.id", id.to_string());
            let session = CurrentSession {
                token: token.to_owned(),
                user: UserBody {
                    id,
                    name,
                    colour: colour.into(),
                    symbol: symbol.into(),
                },
            };
            next.run(&cx.with(session), body).await
        })
    }
}
