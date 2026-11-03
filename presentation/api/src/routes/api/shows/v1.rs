use crate::routes::api::tile::CursorParam;
use crate::routes::api::tile::SortParam;
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
    genre: Option<String>,
    sort: Option<SortParam>,
    cursor: Option<CursorParam>,
}

#[route(GET "/api/(user)/shows/v1")]
pub(crate) async fn shows(cx: &Cx) -> topcoat::Result<Response> {
    let query = query_params::<Query>(cx)?;
    let sort = query.sort.unwrap_or_default();
    let after = query
        .cursor
        .as_ref()
        .map(|x| x.for_sort(sort))
        .transpose()?;
    let page = application::browsing::list_shows::call(
        app_context::<AppState>(cx),
        query.genre.clone(),
        sort.into(),
        after,
    )
    .await?;
    Json(&TilePageBody::new(page.tiles, page.next)).into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::add_show;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use domain::Show;
    use http::Method;
    use http::StatusCode;
    use jiff::civil::date;
    use serde_json::json;
    use test_app::TestApp;
    use uuid::Uuid;

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
    async fn the_genre_and_sort_choose_the_shows_and_their_order() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        for (title, genre, air_date) in [
            ("Andor", "Drama", date(2025, 4, 22)),
            ("Severance", "Drama", date(2025, 3, 21)),
            ("The Bear", "Comedy", date(2025, 6, 25)),
            ("Slow Horses", "Drama", date(2025, 10, 22)),
        ] {
            let show_id = add_show(&app, title, &[]).await;
            db_context
                .shows()
                .filter(Show::fields().id().eq(show_id))
                .update()
                .genres(vec![genre.to_owned()])
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            let season = db_context
                .seasons()
                .create()
                .show_id(show_id)
                .number(1)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            db_context
                .episodes()
                .create()
                .season_id(season.id)
                .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
                .number(1)
                .title("Good News About Hell")
                .air_date(Some(air_date))
                .visible(true)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let server = serve(&app).await;

        let (status, page) = server
            .request(
                Method::GET,
                "/api/shows/v1?genre=Drama&sort=released",
                Some(&token),
                None,
            )
            .await;

        let titles: Vec<_> = page["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["title"].as_str().unwrap())
            .collect();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(titles, ["Slow Horses", "Andor", "Severance"]);
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
