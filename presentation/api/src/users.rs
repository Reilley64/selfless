use crate::user_auth::CurrentSession;
use domain::Colour;
use domain::Symbol;
use domain::User;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::request_context;
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

#[route(GET "/api/me/v1")]
pub(crate) async fn me(cx: &Cx) -> topcoat::Result<Response> {
    Json(UserBody::from(&request_context::<CurrentSession>(cx).user)).into_response(cx)
}
