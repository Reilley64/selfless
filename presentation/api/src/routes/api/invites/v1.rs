use crate::problem;
use crate::problem::EMPTY_PASSWORD;
use crate::problem::INVALID_IDEMPOTENCY_KEY;
use crate::routes::api::user::UserBody;
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
        return INVALID_IDEMPOTENCY_KEY.into_response(cx);
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
            }),
        )
            .into_response(cx),
        Err(error) => problem::from_error(cx, error),
    }
}

#[cfg(test)]
mod tests {
    use crate::test::serve;
    use application::invites::create_invite;
    use application::invites::list_pending_invites;
    use application::invites::send_invite_email;
    use domain::EmailAddress;
    use domain::Password;
    use http::HeaderMap;
    use http::Method;
    use http::StatusCode;
    use jiff::SignedDuration;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;
    use test_fixtures::TestServer;
    use uuid::Uuid;

    const ORIGIN: &str = "https://selfless.example";
    const NAME: &str = "Noor";
    const PASSWORD: &str = "a long passphrase about tea";
    const INVITE_NOT_FOUND: &str = "urn:selfless:problem:invite-not-found";

    async fn send_invite(app: &TestApp) -> String {
        create_invite::call(
            &app.state,
            Uuid::nil(),
            EmailAddress::new("noor@household.example".to_owned()).unwrap(),
            ORIGIN.to_owned(),
        )
        .await
        .unwrap();
        let invite_id = list_pending_invites::call(&app.state)
            .await
            .unwrap()
            .invites[0]
            .id;
        send_invite_email::call(&app.state, invite_id, ORIGIN.to_owned())
            .await
            .unwrap();
        app.mailer.sent().last().unwrap().invite_token(ORIGIN)
    }

    async fn accept(server: &TestServer, token: &str, name: &str) -> (StatusCode, Value) {
        accept_with(server, token, name, PASSWORD, &Uuid::new_v4().to_string()).await
    }

    async fn accept_with(
        server: &TestServer,
        token: &str,
        name: &str,
        password: &str,
        idempotency_key: &str,
    ) -> (StatusCode, Value) {
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", idempotency_key.parse().unwrap());
        server
            .request_with_headers(
                Method::POST,
                "/api/invites/accept/v1",
                None,
                headers,
                Some(json!({ "token": token, "name": name, "password": password })),
            )
            .await
    }

    #[tokio::test]
    async fn an_invited_person_accepts_with_the_link_from_the_email_and_signs_in() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;

        let (status, user) = accept(&server, &token, NAME).await;
        let (signed_in, body) = server
            .request(
                Method::POST,
                "/api/sessions/v1",
                None,
                Some(json!({
                    "name": NAME,
                    "password": PASSWORD,
                    "clientKind": "web",
                    "deviceName": "Firefox on macOS",
                    "clientId": Uuid::new_v4(),
                })),
            )
            .await;

        assert_eq!(app.mailer.sent()[0].recipient, "noor@household.example");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(user["name"], NAME);
        assert_eq!(signed_in, StatusCode::CREATED);
        assert_eq!(body["user"], user);
    }

    #[tokio::test]
    async fn a_link_works_once() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;
        accept(&server, &token, NAME).await;

        let (status, problem) = accept(&server, &token, "Ravi").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(problem["type"], INVITE_NOT_FOUND);
    }

    #[tokio::test]
    async fn a_taken_name_returns_409_and_leaves_the_invite_usable() {
        let app = TestApp::new().await;
        application::users::create_user::call(
            &app.state,
            NAME.to_owned(),
            Password::new("another password".to_owned()).unwrap(),
            EmailAddress::new("noor@example.com".to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let token = send_invite(&app).await;
        let server = serve(&app).await;

        let (status, problem) = accept(&server, &token, NAME).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["type"], "urn:selfless:problem:name-taken");
    }

    #[tokio::test]
    async fn repeating_an_accept_with_the_same_key_returns_the_same_user() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;
        let key = Uuid::new_v4().to_string();

        let first = accept_with(&server, &token, NAME, PASSWORD, &key).await;
        let repeat = accept_with(&server, &token, NAME, PASSWORD, &key).await;

        assert_eq!(first.0, StatusCode::CREATED);
        assert_eq!(repeat, first);
    }

    #[tokio::test]
    async fn a_link_expires_7_days_after_its_email_was_sent() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;
        app.clock.advance(SignedDuration::from_hours(7 * 24));

        let (status, problem) = accept(&server, &token, NAME).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(problem["type"], INVITE_NOT_FOUND);
    }

    #[tokio::test]
    async fn an_empty_password_returns_422() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;

        let (status, problem) =
            accept_with(&server, &token, NAME, "", &Uuid::new_v4().to_string()).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(problem["type"], "urn:selfless:problem:empty-password");
    }

    #[tokio::test]
    async fn accepting_without_a_password_returns_a_bad_request_problem() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "idempotency-key",
            Uuid::new_v4().to_string().parse().unwrap(),
        );

        let (status, problem) = server
            .request_with_headers(
                Method::POST,
                "/api/invites/accept/v1",
                None,
                headers,
                Some(json!({ "token": token, "name": NAME })),
            )
            .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    }

    #[tokio::test]
    async fn an_accept_without_a_uuid_idempotency_key_returns_an_invalid_idempotency_key_problem() {
        let app = TestApp::new().await;
        let token = send_invite(&app).await;
        let server = serve(&app).await;

        let missing = server
            .request(
                Method::POST,
                "/api/invites/accept/v1",
                None,
                Some(json!({ "token": token, "name": NAME, "password": PASSWORD })),
            )
            .await;
        let not_uuid = accept_with(&server, &token, NAME, PASSWORD, "not-a-uuid").await;

        for (status, problem) in [missing, not_uuid] {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(
                problem["type"],
                "urn:selfless:problem:invalid-idempotency-key"
            );
        }
    }
}
