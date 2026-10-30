use crate::problem::WRONG_NAME_OR_PASSWORD;
use crate::routes::api::user::UserBody;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignIn {
    name: String,
    password: String,
    client_kind: ClientKind,
    device_name: String,
    client_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum ClientKind {
    Web,
    AppleTv,
    GoogleTv,
}

impl From<ClientKind> for domain::ClientKind {
    fn from(client_kind: ClientKind) -> Self {
        match client_kind {
            ClientKind::Web => Self::Web,
            ClientKind::AppleTv => Self::AppleTv,
            ClientKind::GoogleTv => Self::GoogleTv,
        }
    }
}

#[derive(Serialize)]
struct SignedIn {
    token: String,
    user: UserBody,
}

#[route(POST "/api/(public)/sessions/v1")]
pub(crate) async fn sign_in(cx: &Cx, Json(request): Json<SignIn>) -> topcoat::Result<Response> {
    let application::sessions::sign_in::Output::SignedIn {
        token,
        user_id,
        name,
        colour,
        symbol,
    } = application::sessions::sign_in::call(
        app_context::<AppState>(cx),
        request.name,
        request.password,
        request.client_kind.into(),
        request.device_name,
        request.client_id,
    )
    .await?
    else {
        return WRONG_NAME_OR_PASSWORD.into_response(cx);
    };
    (
        StatusCode::CREATED,
        Json(SignedIn {
            token,
            user: UserBody {
                id: user_id,
                name,
                colour: colour.into(),
                symbol: symbol.into(),
            },
        }),
    )
        .into_response(cx)
}

#[route(DELETE "/api/(user)/sessions/current/v1")]
pub(crate) async fn sign_out(cx: &Cx) -> topcoat::Result<Response> {
    let token = &request_context::<CurrentSession>(cx).token;
    application::sessions::sign_out::call(app_context::<AppState>(cx), token).await?;
    StatusCode::NO_CONTENT.into_response(cx)
}
