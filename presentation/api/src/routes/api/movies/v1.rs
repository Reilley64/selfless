use crate::routes::api::tile::CursorParam;
use crate::routes::api::tile::TilePageBody;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::content::Json;
use topcoat::router::query_params;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[query_params(error = bad_request)]
struct Query {
    cursor: Option<CursorParam>,
}

#[route(GET "/api/(user)/movies/v1")]
pub(crate) async fn movies(cx: &Cx) -> topcoat::Result<Response> {
    let query = query_params::<Query>(cx)?;
    let page = application::browsing::list_movies::call(
        app_context::<AppState>(cx),
        query.cursor.as_ref().map(|x| x.0.clone()),
    )
    .await?;
    Json(&TilePageBody::new(page.tiles, page.next)).into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::add_movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use http::Method;
    use http::StatusCode;
    use serde_json::json;
    use test_app::TestApp;

    #[tokio::test]
    async fn a_movie_tile_carries_what_the_focused_details_need() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let movie_id = add_movie(&app, "Arrival").await;
        let server = serve(&app).await;

        let (status, page) = server
            .request(Method::GET, "/api/movies/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            page,
            json!({
                "tiles": [{
                    "id": movie_id,
                    "kind": "movie",
                    "title": "Arrival",
                    "year": 2016,
                    "certification": "PG-13",
                    "runtimeMinutes": 116,
                    "seasonCount": null,
                    "genres": ["Drama", "Science Fiction"],
                    "plot": "A linguist works with the military to talk to aliens.",
                    "images": { "fanart": null, "logo": null, "thumb": null },
                    "progress": null,
                    "watched": false,
                    "episode": null,
                }],
                "nextCursor": null,
            })
        );
    }

    #[tokio::test]
    async fn the_next_cursor_fetches_the_next_page() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        for number in 0..61 {
            add_movie(&app, &format!("Movie {number:02}")).await;
        }
        let server = serve(&app).await;

        let (_, first) = server
            .request(Method::GET, "/api/movies/v1", Some(&token), None)
            .await;
        let next = format!(
            "/api/movies/v1?cursor={}",
            first["nextCursor"].as_str().unwrap()
        );
        let (status, second) = server.request(Method::GET, &next, Some(&token), None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["tiles"][0]["title"], "Movie 60");
        assert_eq!(second["nextCursor"], json!(null));
    }

    #[tokio::test]
    async fn the_movies_grid_needs_a_signed_in_user() {
        let app = TestApp::new().await;
        let server = serve(&app).await;

        let (status, problem) = server.get("/api/movies/v1").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(problem["type"], "urn:selfless:problem:unauthenticated");
    }

    #[tokio::test]
    async fn a_cursor_the_server_did_not_issue_returns_a_bad_request_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let server = serve(&app).await;

        let (status, problem) = server
            .request(
                Method::GET,
                "/api/movies/v1?cursor=not-a-cursor",
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    }
}
