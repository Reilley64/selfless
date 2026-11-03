use crate::problem;
use crate::user_auth::CurrentSession;
use domain::Password;
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackSettingsBody {
    autoplay_next_episode: bool,
    ask_if_still_watching: bool,
}

#[route(GET "/api/(user)/me/playback-settings/v1")]
pub(crate) async fn playback_settings(cx: &Cx) -> topcoat::Result<Response> {
    let settings = application::playback::get_playback_settings::call(
        app_context::<AppState>(cx),
        request_context::<CurrentSession>(cx).user.id,
    )
    .await?;
    Json(&PlaybackSettingsBody {
        autoplay_next_episode: settings.autoplay_next_episode,
        ask_if_still_watching: settings.ask_if_still_watching,
    })
    .into_response(cx)
}

#[route(PUT "/api/(user)/me/playback-settings/v1")]
pub(crate) async fn change_playback_settings(
    cx: &Cx,
    Json(request): Json<PlaybackSettingsBody>,
) -> topcoat::Result<Response> {
    let settings = application::playback::change_playback_settings::call(
        app_context::<AppState>(cx),
        request_context::<CurrentSession>(cx).user.id,
        request.autoplay_next_episode,
        request.ask_if_still_watching,
    )
    .await?;
    Json(&PlaybackSettingsBody {
        autoplay_next_episode: settings.autoplay_next_episode,
        ask_if_still_watching: settings.ask_if_still_watching,
    })
    .into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::serve;
    use crate::test::signed_in_user;
    use http::Method;
    use http::StatusCode;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;
    use test_fixtures::TestServer;

    const NAME: &str = "Noor";
    const PASSWORD: &str = "a long passphrase about tea";

    async fn change_password(server: &TestServer, token: &str, body: Value) -> (StatusCode, Value) {
        server
            .request(Method::PUT, "/api/me/password/v1", Some(token), Some(body))
            .await
    }

    #[tokio::test]
    async fn me_returns_the_signed_in_user() {
        let app = TestApp::new().await;
        let (user_id, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;

        let (status, me) = server
            .request(Method::GET, "/api/me/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(me["id"], user_id.to_string());
        assert_eq!(me["name"], NAME);
    }

    #[tokio::test]
    async fn changing_the_password_returns_204_and_keeps_the_calling_session() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;

        let (status, _) = change_password(
            &server,
            &token,
            json!({ "currentPassword": PASSWORD, "newPassword": "a new passphrase" }),
        )
        .await;
        let (me, _) = server
            .request(Method::GET, "/api/me/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(me, StatusCode::OK);
    }

    #[tokio::test]
    async fn a_wrong_current_password_returns_403_wrong_current_password() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;

        let (status, problem) = change_password(
            &server,
            &token,
            json!({ "currentPassword": "not the password", "newPassword": "a new passphrase" }),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            problem["type"],
            "urn:selfless:problem:wrong-current-password"
        );
    }

    #[tokio::test]
    async fn an_empty_new_password_returns_422_empty_password() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;

        let (status, problem) = change_password(
            &server,
            &token,
            json!({ "currentPassword": PASSWORD, "newPassword": "" }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(problem["type"], "urn:selfless:problem:empty-password");
    }

    #[tokio::test]
    async fn changed_playback_settings_are_read_back() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;
        let changed = json!({ "autoplayNextEpisode": false, "askIfStillWatching": true });

        let (status, response) = server
            .request(
                Method::PUT,
                "/api/me/playback-settings/v1",
                Some(&token),
                Some(changed.clone()),
            )
            .await;
        let (_, settings) = server
            .request(
                Method::GET,
                "/api/me/playback-settings/v1",
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, changed);
        assert_eq!(settings, changed);
    }

    #[tokio::test]
    async fn a_password_change_without_the_current_password_returns_a_bad_request_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let server = serve(&app).await;

        let (status, problem) = change_password(
            &server,
            &token,
            json!({ "newPassword": "a new passphrase" }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    }
}
