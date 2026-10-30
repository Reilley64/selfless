use crate::problem;
use crate::problem::BAD_REQUEST;
use crate::problem::EMPTY_PASSWORD;
use crate::routes::api::me::v1::UserBody;
use domain::Password;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::request::headers;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

#[derive(Deserialize)]
struct AcceptInvite {
    token: String,
    name: String,
    password: String,
}

#[route(POST "/api/(public)/invites/accept/v1")]
pub(crate) async fn accept(
    cx: &Cx,
    Json(request): Json<AcceptInvite>,
) -> topcoat::Result<Response> {
    let Some(idempotency_key) = headers(cx)
        .get("idempotency-key")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| Uuid::try_parse(x).ok())
    else {
        return BAD_REQUEST.into_response(cx);
    };
    let Ok(password) = Password::new(request.password) else {
        return EMPTY_PASSWORD.into_response(cx);
    };
    match application::invites::accept_invite::call(
        app_context::<AppState>(cx),
        &request.token,
        request.name,
        password,
        idempotency_key,
    )
    .await
    {
        Ok(output) => (
            StatusCode::CREATED,
            Json(UserBody {
                id: output.id,
                name: output.name,
                colour: output.colour.into(),
                symbol: output.symbol.into(),
            }),
        )
            .into_response(cx),
        Err(error) => problem::from_error(cx, error),
    }
}
