use crate::problem;
use crate::user_auth::CurrentSession;
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
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) colour: Colour,
    pub(crate) symbol: Symbol,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl From<domain::Colour> for Colour {
    fn from(colour: domain::Colour) -> Self {
        match colour {
            domain::Colour::Amber => Self::Amber,
            domain::Colour::Blue => Self::Blue,
            domain::Colour::Pink => Self::Pink,
            domain::Colour::Emerald => Self::Emerald,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Symbol {
    Ring,
    Triangle,
    Star,
    Waves,
}

impl From<domain::Symbol> for Symbol {
    fn from(symbol: domain::Symbol) -> Self {
        match symbol {
            domain::Symbol::Ring => Self::Ring,
            domain::Symbol::Triangle => Self::Triangle,
            domain::Symbol::Star => Self::Star,
            domain::Symbol::Waves => Self::Waves,
        }
    }
}

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
