use crate::problem::NOT_FOUND;
use crate::user_auth::CurrentSession;
use application::browsing::remove_movie_from_continue_watching;
use application::browsing::remove_show_from_continue_watching;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::StatusCode;
use topcoat::router::path_param;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

path_param!(movie_id: Uuid);
path_param!(show_id: Uuid);

#[route(DELETE "/api/(user)/continue-watching/movies/{movie_id}/v1")]
pub(crate) async fn remove_movie(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(movie_id) = path_param::<MovieId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    let user_id = request_context::<CurrentSession>(cx).user.id;
    match remove_movie_from_continue_watching::call(app_context::<AppState>(cx), user_id, *movie_id)
        .await?
    {
        remove_movie_from_continue_watching::Output::Removed => {
            StatusCode::NO_CONTENT.into_response(cx)
        }
        remove_movie_from_continue_watching::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[route(DELETE "/api/(user)/continue-watching/shows/{show_id}/v1")]
pub(crate) async fn remove_show(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(show_id) = path_param::<ShowId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    let user_id = request_context::<CurrentSession>(cx).user.id;
    match remove_show_from_continue_watching::call(app_context::<AppState>(cx), user_id, *show_id)
        .await?
    {
        remove_show_from_continue_watching::Output::Removed => {
            StatusCode::NO_CONTENT.into_response(cx)
        }
        remove_show_from_continue_watching::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[cfg(test)]
mod tests {
    use crate::test::add_media_file;
    use crate::test::add_movie;
    use crate::test::add_show;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use crate::test::stop_at;
    use application::ports::persistence::DbScope;
    use domain::Episode;
    use http::Method;
    use http::StatusCode;
    use jiff::SignedDuration;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;
    use test_fixtures::TestServer;
    use uuid::Uuid;

    async fn continue_watching(server: &TestServer, token: &str) -> Vec<Value> {
        let (_, home) = server
            .request(Method::GET, "/api/home/v1", Some(token), None)
            .await;
        home["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["kind"] == "continueWatching")
            .flat_map(|x| x["tiles"].as_array().unwrap().clone())
            .map(|x| x["title"].clone())
            .collect()
    }

    #[tokio::test]
    async fn removing_a_movie_and_a_show_returns_204_and_leaves_them_out_of_continue_watching() {
        let app = TestApp::new().await;
        let (noor, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let movie_file = add_media_file(&app, Some(3_000_000)).await;
        let arrival = movie(&app, "Arrival")
            .media_file_id(Some(movie_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let dune = add_movie(&app, "Dune").await;
        let severance = add_show(&app, "Severance", &[1]).await;
        let episode_file = add_media_file(&app, Some(3_000_000)).await;
        db_context
            .episodes()
            .all()
            .update()
            .media_file_id(Some(episode_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for file in [movie_file, episode_file] {
            stop_at(&app, noor, file, SignedDuration::from_mins(15)).await;
        }
        let server = serve(&app).await;
        let before = continue_watching(&server, &token).await;
        app.clock.advance(SignedDuration::from_mins(1));

        let mut statuses = Vec::new();
        for path in [
            format!("/api/continue-watching/movies/{arrival}/v1"),
            format!("/api/continue-watching/shows/{severance}/v1"),
            format!("/api/continue-watching/movies/{dune}/v1"),
        ] {
            let (status, _) = server
                .request(Method::DELETE, &path, Some(&token), None)
                .await;
            statuses.push(status);
        }

        assert_eq!(before.len(), 2);
        assert_eq!(statuses, [StatusCode::NO_CONTENT; 3]);
        assert_eq!(
            continue_watching(&server, &token).await,
            Vec::<Value>::new()
        );
    }

    #[tokio::test]
    async fn removing_an_unknown_or_hidden_title_returns_404() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let gone = movie(&app, "Gone")
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let hidden_show = add_show(&app, "Severance", &[1]).await;
        db_context
            .episodes()
            .filter(Episode::fields().in_library().eq(true))
            .update()
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        for path in [
            format!("/api/continue-watching/movies/{gone}/v1"),
            format!("/api/continue-watching/movies/{}/v1", Uuid::new_v4()),
            "/api/continue-watching/movies/arrival/v1".to_owned(),
            format!("/api/continue-watching/shows/{hidden_show}/v1"),
            format!("/api/continue-watching/shows/{}/v1", Uuid::new_v4()),
        ] {
            let (status, problem) = server
                .request(Method::DELETE, &path, Some(&token), None)
                .await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(problem["type"], json!("urn:selfless:problem:not-found"));
        }
    }
}
