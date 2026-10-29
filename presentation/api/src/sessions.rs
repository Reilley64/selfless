use crate::problem::WRONG_NAME_OR_PASSWORD;
use crate::user_auth::CurrentSession;
use crate::users::UserBody;
use domain::ClientKind;
use infrastructure::AppState;
use serde::Deserialize;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignIn {
    name: String,
    password: String,
    client_kind: ClientKind,
    device_name: String,
}

#[derive(Serialize)]
struct SignedIn {
    token: String,
    user: UserBody,
}

#[route(POST "/api/sessions/v1")]
pub(crate) async fn sign_in(cx: &Cx, Json(request): Json<SignIn>) -> topcoat::Result<Response> {
    let signed_in = application::sign_in::call(
        app_context::<AppState>(cx),
        request.name,
        request.password,
        request.client_kind,
        request.device_name,
    )
    .await?;
    let Some(signed_in) = signed_in else {
        return WRONG_NAME_OR_PASSWORD.into_response(cx);
    };
    (
        StatusCode::CREATED,
        Json(SignedIn {
            token: signed_in.token,
            user: UserBody::from(&signed_in.user),
        }),
    )
        .into_response(cx)
}

#[route(DELETE "/api/sessions/current/v1")]
pub(crate) async fn sign_out(cx: &Cx) -> topcoat::Result<Response> {
    let token = &request_context::<CurrentSession>(cx).token;
    application::sign_out::call(app_context::<AppState>(cx), token).await?;
    StatusCode::NO_CONTENT.into_response(cx)
}
