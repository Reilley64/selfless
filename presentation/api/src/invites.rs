use crate::problem;
use crate::users::UserBody;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

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
    match application::accept_invite::call(
        app_context::<AppState>(cx),
        &request.token,
        request.name,
        request.password,
    )
    .await
    {
        Ok(user) => (StatusCode::CREATED, Json(UserBody::from(&user))).into_response(cx),
        Err(error) => problem::from_error(cx, error),
    }
}
