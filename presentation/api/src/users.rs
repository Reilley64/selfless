use crate::problem;
use crate::user_auth::CurrentSession;
use domain::Colour;
use domain::Symbol;
use domain::User;
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
use uuid::Uuid;

#[derive(Serialize)]
pub(crate) struct UserBody {
    id: Uuid,
    name: String,
    colour: Colour,
    symbol: Symbol,
}

impl From<&User> for UserBody {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            name: user.name.clone(),
            colour: user.colour,
            symbol: user.symbol,
        }
    }
}

#[route(GET "/api/(user)/me/v1")]
pub(crate) async fn me(cx: &Cx) -> topcoat::Result<Response> {
    Json(UserBody::from(&request_context::<CurrentSession>(cx).user)).into_response(cx)
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
    match application::users::change_password::call(
        app_context::<AppState>(cx),
        session.user.id,
        &session.token,
        request.current_password,
        request.new_password,
    )
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(cx),
        Err(error) => problem::from_error(cx, error),
    }
}
