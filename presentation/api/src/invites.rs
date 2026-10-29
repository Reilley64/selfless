use crate::problem::EMPTY_PASSWORD;
use crate::problem::INVITE_NOT_FOUND;
use crate::problem::NAME_TAKEN;
use crate::users::UserBody;
use application::EmptyPassword;
use application::accept_invite::InviteNotFound;
use application::create_user::NameTaken;
use infrastructure::AppState;
use ohno::ErrorExt;
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

#[route(POST "/(public)/api/invites/accept/v1")]
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
        Err(error) if error.find_source::<InviteNotFound>().is_some() => {
            INVITE_NOT_FOUND.into_response(cx)
        }
        Err(error) if error.find_source::<NameTaken>().is_some() => NAME_TAKEN.into_response(cx),
        Err(error) if error.find_source::<EmptyPassword>().is_some() => {
            EMPTY_PASSWORD.into_response(cx)
        }
        Err(error) => Err(error.into()),
    }
}
