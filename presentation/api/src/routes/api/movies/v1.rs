use crate::problem::NOT_FOUND;
use crate::routes::api::image::TitleImagesBody;
use crate::routes::api::tile::CursorParam;
use crate::routes::api::tile::SortParam;
use crate::routes::api::tile::TileBody;
use crate::routes::api::tile::TilePageBody;
use crate::user_auth::CurrentSession;
use application::browsing::get_movie;
use infrastructure::AppState;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::content::Json;
use topcoat::router::path_param;
use topcoat::router::query_params;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

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
        request_context::<CurrentSession>(cx).user.id,
        query.genre.clone(),
        sort.into(),
        after,
    )
    .await?;
    Json(&TilePageBody::new(page.tiles, page.next)).into_response(cx)
}

path_param!(movie_id: Uuid);

#[route(GET "/api/(user)/movies/{movie_id}/v1")]
pub(crate) async fn movie(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(movie_id) = path_param::<MovieId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    let user_id = request_context::<CurrentSession>(cx).user.id;
    match get_movie::call(app_context::<AppState>(cx), user_id, *movie_id).await? {
        get_movie::Output::Movie(page) => Json(&MoviePageBody::from(*page)).into_response(cx),
        get_movie::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoviePageBody {
    id: Uuid,
    title: String,
    year: i64,
    certification: Option<String>,
    runtime_minutes: Option<i64>,
    genres: Vec<String>,
    rating: Option<f64>,
    plot: Option<String>,
    studio: Option<String>,
    original_title: Option<String>,
    directors: Vec<String>,
    cast: Vec<CastMemberBody>,
    images: TitleImagesBody,
    progress: Option<f64>,
    watched: bool,
    play: MoviePlayBody,
    more_like_this: Vec<TileBody>,
}

#[derive(Serialize)]
struct CastMemberBody {
    name: String,
    role: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoviePlayBody {
    resume_position: Option<f64>,
}

impl From<get_movie::Movie> for MoviePageBody {
    fn from(page: get_movie::Movie) -> Self {
        Self {
            id: page.id,
            title: page.title,
            year: page.year,
            certification: page.certification,
            runtime_minutes: page.runtime_minutes,
            genres: page.genres,
            rating: page.rating,
            plot: page.plot,
            studio: page.studio,
            original_title: page.original_title,
            directors: page.directors,
            cast: page
                .cast
                .into_iter()
                .map(|x| CastMemberBody {
                    name: x.name,
                    role: x.role,
                })
                .collect(),
            images: TitleImagesBody::from(page.images),
            progress: page.progress,
            watched: page.watched,
            play: MoviePlayBody {
                resume_position: page.resume_position_seconds.map(|x| x as f64),
            },
            more_like_this: page
                .more_like_this
                .into_iter()
                .map(TileBody::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test::add_artwork;
    use crate::test::add_media_file;
    use crate::test::add_movie;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use crate::test::stop_at;
    use application::ports::persistence::DbScope;
    use domain::ArtworkKind;
    use http::Method;
    use http::StatusCode;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use jiff::ToSpan;
    use jiff::civil::date;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;
    use uuid::Uuid;

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
    async fn the_movie_page_carries_the_metadata_credits_artwork_play_target_and_more_like_this() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let arrival = movie(&app, "Arrival")
            .original_title(Some("Arrival".to_owned()))
            .rating(Some(7.6))
            .studio(Some("Paramount".to_owned()))
            .directors(vec!["Denis Villeneuve".to_owned()])
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        db_context
            .actors()
            .create()
            .movie_id(Some(arrival.id))
            .name("Amy Adams")
            .role(Some("Louise Banks".to_owned()))
            .order(Some(0))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        add_artwork(
            &app,
            arrival.id,
            ArtworkKind::Fanart,
            "/movies/Arrival/fanart.jpg",
        )
        .await;
        let dune = add_movie(&app, "Dune").await;
        let server = serve(&app).await;

        let (status, mut page) = server
            .request(
                Method::GET,
                &format!("/api/movies/{}/v1", arrival.id),
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        let fanart = page["images"]["fanart"].take();
        assert_eq!(
            fanart["urls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["width"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [480, 960, 1280, 1920]
        );
        assert_eq!(page["moreLikeThis"][0]["id"], json!(dune));
        page["moreLikeThis"].take();
        assert_eq!(
            page,
            json!({
                "id": arrival.id,
                "title": "Arrival",
                "year": 2016,
                "certification": "PG-13",
                "runtimeMinutes": 116,
                "genres": ["Drama", "Science Fiction"],
                "rating": 7.6,
                "plot": "A linguist works with the military to talk to aliens.",
                "studio": "Paramount",
                "originalTitle": "Arrival",
                "directors": ["Denis Villeneuve"],
                "cast": [{ "name": "Amy Adams", "role": "Louise Banks" }],
                "images": { "fanart": null, "logo": null },
                "progress": null,
                "watched": false,
                "play": { "resumePosition": null },
                "moreLikeThis": null,
            })
        );
    }

    #[tokio::test]
    async fn the_movie_page_and_its_tiles_carry_the_signed_in_users_watch_state() {
        let app = TestApp::new().await;
        let (noor, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let (_, ravi) = signed_in_user(&app, "Ravi", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let arrival_file = add_media_file(&app, Some(2_400_000)).await;
        let arrival = movie(&app, "Arrival")
            .media_file_id(Some(arrival_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let sicario_file = add_media_file(&app, Some(6_000_000)).await;
        movie(&app, "Sicario")
            .media_file_id(Some(sicario_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        stop_at(&app, noor, arrival_file, SignedDuration::from_mins(20)).await;
        stop_at(&app, noor, sicario_file, SignedDuration::from_mins(50)).await;
        let server = serve(&app).await;

        let mut views = Vec::new();
        for token in [&token, &ravi] {
            let (status, page) = server
                .request(
                    Method::GET,
                    &format!("/api/movies/{}/v1", arrival.id),
                    Some(token),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK);
            views.push(json!([
                page["progress"],
                page["watched"],
                page["play"],
                page["moreLikeThis"][0]["progress"],
                page["moreLikeThis"][0]["watched"],
            ]));
        }

        assert_eq!(
            views,
            [
                json!([0.5, false, { "resumePosition": 1200.0 }, null, true]),
                json!([null, false, { "resumePosition": null }, null, false]),
            ]
        );
    }

    #[tokio::test]
    async fn an_unknown_out_of_library_or_malformed_movie_id_returns_the_not_found_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let out_of_library = movie(&app, "Arrival")
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        for id in [
            out_of_library.id.to_string(),
            Uuid::new_v4().to_string(),
            "arrival".to_owned(),
        ] {
            let (status, problem) = server
                .request(
                    Method::GET,
                    &format!("/api/movies/{id}/v1"),
                    Some(&token),
                    None,
                )
                .await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(problem["type"], "urn:selfless:problem:not-found", "{id}");
        }
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
