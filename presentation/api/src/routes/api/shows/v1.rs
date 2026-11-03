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

#[route(GET "/api/(user)/shows/v1")]
pub(crate) async fn shows(cx: &Cx) -> topcoat::Result<Response> {
    let query = query_params::<Query>(cx)?;
    let page = application::browsing::list_shows::call(
        app_context::<AppState>(cx),
        query.cursor.as_ref().map(|x| x.0.clone()),
    )
    .await?;
    Json(&TilePageBody::new(page.tiles, page.next)).into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::add_show;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use http::Method;
    use http::StatusCode;
    use serde_json::json;
    use test_app::TestApp;

    #[tokio::test]
    async fn a_show_tile_carries_what_the_focused_details_need() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let show_id = add_show(&app, "Severance", &[1, 2]).await;
        let server = serve(&app).await;

        let (status, page) = server
            .request(Method::GET, "/api/shows/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            page,
            json!({
                "tiles": [{
                    "id": show_id,
                    "kind": "show",
                    "title": "Severance",
                    "year": 2022,
                    "certification": "TV-MA",
                    "runtimeMinutes": null,
                    "seasonCount": 2,
                    "genres": ["Drama", "Mystery"],
                    "plot": "Office workers have their memories split.",
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
    async fn the_shows_grid_needs_a_signed_in_user() {
        let app = TestApp::new().await;
        let server = serve(&app).await;

        let (status, problem) = server.get("/api/shows/v1").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(problem["type"], "urn:selfless:problem:unauthenticated");
    }
}
