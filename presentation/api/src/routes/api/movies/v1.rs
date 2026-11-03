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

#[route(GET "/api/(user)/movies/v1")]
pub(crate) async fn movies(cx: &Cx) -> topcoat::Result<Response> {
    let query = query_params::<Query>(cx)?;
    let sort = query.sort.unwrap_or_default();
    let after = query
        .cursor
        .as_ref()
        .map(|x| x.for_sort(sort))
        .transpose()?;
    let page = application::browsing::list_movies::call(
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
    use crate::test::add_movie;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use http::Method;
    use http::StatusCode;
    use jiff::Timestamp;
    use jiff::ToSpan;
    use jiff::civil::date;
    use serde_json::Value;
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

    async fn add_61_dated_movies(app: &TestApp) {
        let mut db_context = app.state.db_context.clone();
        for number in 0..61 {
            let added_at = (number < 60).then(|| Timestamp::from_second(number * 86_400).unwrap());
            let premiered = (number < 59).then(|| date(2000, 1, 1) + number.days());
            movie(app, &format!("Movie {number:02}"))
                .added_at(added_at)
                .premiered(premiered)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
    }

    fn titles(page: &Value) -> Vec<String> {
        page["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["title"].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn the_next_cursor_fetches_the_next_page_under_every_sort() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        add_61_dated_movies(&app).await;
        let server = serve(&app).await;

        for (sort, last) in [
            ("title", Some("Movie 60")),
            ("added", Some("Movie 60")),
            ("released", None),
        ] {
            let path = format!("/api/movies/v1?sort={sort}");
            let (_, first) = server.request(Method::GET, &path, Some(&token), None).await;
            let next = format!("{path}&cursor={}", first["nextCursor"].as_str().unwrap());
            let (status, second) = server.request(Method::GET, &next, Some(&token), None).await;

            let mut all = [titles(&first), titles(&second)].concat();
            assert_eq!(status, StatusCode::OK, "{sort}");
            assert_eq!(titles(&second).len(), 1, "{sort}");
            if let Some(last) = last {
                assert_eq!(titles(&second), [last], "{sort}");
            }
            assert_eq!(second["nextCursor"], json!(null), "{sort}");
            all.sort();
            all.dedup();
            assert_eq!(all.len(), 61, "{sort}");
        }
    }

    #[tokio::test]
    async fn the_genre_and_sort_choose_the_movies_and_their_order() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        for (title, genre, added_at) in [
            ("Arrival", "Drama", "2026-10-29T08:00:00Z"),
            ("Dune", "Science Fiction", "2026-10-30T08:00:00Z"),
            ("Sicario", "Drama", "2026-10-22T08:00:00Z"),
            ("Blade Runner 2049", "Drama", "2026-10-21T08:00:00Z"),
        ] {
            movie(&app, title)
                .genres(vec![genre.to_owned()])
                .added_at(Some(added_at.parse::<Timestamp>().unwrap()))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let server = serve(&app).await;

        let (status, page) = server
            .request(
                Method::GET,
                "/api/movies/v1?genre=Drama&sort=added",
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(titles(&page), ["Arrival", "Sicario", "Blade Runner 2049"]);
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
    async fn an_unknown_sort_or_a_cursor_the_server_did_not_issue_for_the_sort_returns_a_bad_request_problem()
     {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        add_61_dated_movies(&app).await;
        let server = serve(&app).await;
        let (_, first) = server
            .request(Method::GET, "/api/movies/v1", Some(&token), None)
            .await;
        let title_cursor = first["nextCursor"].as_str().unwrap();

        for path in [
            "/api/movies/v1?cursor=not-a-cursor".to_owned(),
            "/api/movies/v1?sort=popular".to_owned(),
            format!("/api/movies/v1?sort=added&cursor={title_cursor}"),
        ] {
            let (status, problem) = server.request(Method::GET, &path, Some(&token), None).await;

            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
            assert_eq!(
                problem["type"], "urn:selfless:problem:bad-request",
                "{path}"
            );
        }
    }
}
