use crate::problem;
use crate::user_auth::CurrentSession;
use domain::Password;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route(GET "/api/(user)/me/v1")]
pub(crate) async fn me(cx: &Cx) -> topcoat::Result<Response> {
    Json(&request_context::<CurrentSession>(cx).user).into_response(cx)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePassword {
    current_password: String,
    new_password: String,
}

#[route(PUT "/api/(user)/me/password/v1")]
pub(crate) async fn change_password(
    cx: &Cx,
    Json(request): Json<ChangePassword>,
) -> topcoat::Result<Response> {
    let session = request_context::<CurrentSession>(cx);
    let Ok(new_password) = Password::new(request.new_password) else {
        return problem::EMPTY_PASSWORD.into_response(cx);
    };
    match application::users::change_password::call(
        app_context::<AppState>(cx),
        session.user.id,
        &session.token,
        request.current_password,
        new_password,
    )
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(cx),
        Err(error) => problem::from_error(cx, error),
    }
}
