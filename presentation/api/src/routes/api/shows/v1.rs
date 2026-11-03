use crate::problem::NOT_FOUND;
use crate::routes::api::image::TitleImagesBody;
use crate::routes::api::tile::CursorParam;
use crate::routes::api::tile::SortParam;
use crate::routes::api::tile::TileBody;
use crate::routes::api::tile::TilePageBody;
use crate::user_auth::CurrentSession;
use application::browsing::get_show;
use domain::PlayVerb;
use domain::Season;
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

path_param!(show_id: Uuid);

#[route(GET "/api/(user)/shows/{show_id}/v1")]
pub(crate) async fn show(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(show_id) = path_param::<ShowId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    let user_id = request_context::<CurrentSession>(cx).user.id;
    match get_show::call(app_context::<AppState>(cx), user_id, *show_id).await? {
        get_show::Output::Show(page) => Json(&ShowPageBody::from(*page)).into_response(cx),
        get_show::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowPageBody {
    id: Uuid,
    title: String,
    year: Option<i64>,
    certification: Option<String>,
    status: Option<String>,
    network: Option<String>,
    season_count: i64,
    genres: Vec<String>,
    rating: Option<f64>,
    plot: Option<String>,
    cast: Vec<CastMemberBody>,
    images: TitleImagesBody,
    seasons: Vec<SeasonBody>,
    play: PlayBody,
    more_like_this: Vec<TileBody>,
}

#[derive(Serialize)]
struct CastMemberBody {
    name: String,
    role: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SeasonBody {
    id: Uuid,
    number: i64,
    name: String,
    episode_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayBody {
    verb: Verb,
    resume_position: Option<f64>,
    season: PlaySeasonBody,
    episode: PlayEpisodeBody,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Verb {
    Play,
    Resume,
}

#[derive(Serialize)]
struct PlaySeasonBody {
    id: Uuid,
    number: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayEpisodeBody {
    id: Uuid,
    number: i64,
    title: String,
    media_file_id: Option<Uuid>,
}

impl From<get_show::Show> for ShowPageBody {
    fn from(page: get_show::Show) -> Self {
        Self {
            id: page.id,
            title: page.title,
            year: page.year,
            certification: page.certification,
            status: page.status,
            network: page.network,
            season_count: page.season_count,
            genres: page.genres,
            rating: page.rating,
            plot: page.plot,
            cast: page
                .cast
                .into_iter()
                .map(|x| CastMemberBody {
                    name: x.name,
                    role: x.role,
                })
                .collect(),
            images: TitleImagesBody::from(page.images),
            seasons: page
                .seasons
                .into_iter()
                .map(|x| SeasonBody {
                    id: x.id,
                    number: x.number,
                    name: if Season::is_specials(x.number) {
                        "Specials".to_owned()
                    } else {
                        format!("Season {}", x.number)
                    },
                    episode_count: x.episode_count,
                })
                .collect(),
            play: PlayBody {
                verb: match page.play.verb {
                    PlayVerb::Play => Verb::Play,
                    PlayVerb::Resume { .. } => Verb::Resume,
                },
                resume_position: match page.play.verb {
                    PlayVerb::Play => None,
                    PlayVerb::Resume { position_seconds } => Some(position_seconds as f64),
                },
                season: PlaySeasonBody {
                    id: page.play.season_id,
                    number: page.play.season_number,
                },
                episode: PlayEpisodeBody {
                    id: page.play.episode_id,
                    number: page.play.episode_number,
                    title: page.play.episode_title,
                    media_file_id: page.play.media_file_id,
                },
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
    use crate::test::add_media_file;
    use crate::test::add_show;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use crate::test::stop_at;
    use application::ports::persistence::DbScope;
    use domain::ArtworkKind;
    use domain::Episode;
    use domain::Season;
    use domain::Show;
    use http::Method;
    use http::StatusCode;
    use jiff::SignedDuration;
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
                .tvdb_id(domain::TvdbId(Uuid::new_v4().as_u64_pair().0 as i64))
                .number(1)
                .title("Good News About Hell")
                .air_date(Some(air_date))
                .in_library(true)
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

    #[tokio::test]
    async fn the_show_page_carries_the_metadata_cast_artwork_seasons_play_target_and_more_like_this()
     {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let severance = add_show(&app, "Severance", &[0, 1]).await;
        let andor = add_show(&app, "Andor", &[1]).await;
        db_context
            .actors()
            .create()
            .show_id(Some(severance))
            .name("Adam Scott")
            .role(Some("Mark Scout".to_owned()))
            .order(Some(0))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        db_context
            .artwork()
            .create()
            .show_id(Some(severance))
            .kind(ArtworkKind::Clearlogo)
            .path("/media/TV/Severance/clearlogo.png")
            .modified_at(app.clock.now())
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let seasons = db_context
            .seasons()
            .filter(Season::fields().show_id().eq(severance))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = |number| seasons.iter().find(|x| x.number == number).unwrap().id;
        let first_episode = db_context
            .episodes()
            .filter(Episode::fields().season_id().eq(season_id(1)))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .id;
        let server = serve(&app).await;

        let (status, mut page) = server
            .request(
                Method::GET,
                &format!("/api/shows/{severance}/v1"),
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        let logo = page["images"]["logo"].take();
        assert_eq!(
            logo["urls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["width"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [300, 600]
        );
        assert_eq!(page["moreLikeThis"][0]["id"], json!(andor));
        page["moreLikeThis"].take();
        assert_eq!(
            page,
            json!({
                "id": severance,
                "title": "Severance",
                "year": 2022,
                "certification": "TV-MA",
                "status": null,
                "network": null,
                "seasonCount": 1,
                "genres": ["Drama", "Mystery"],
                "rating": null,
                "plot": "Office workers have their memories split.",
                "cast": [{ "name": "Adam Scott", "role": "Mark Scout" }],
                "images": { "fanart": null, "logo": null },
                "seasons": [
                    { "id": season_id(1), "number": 1, "name": "Season 1", "episodeCount": 1 },
                    { "id": season_id(0), "number": 0, "name": "Specials", "episodeCount": 1 },
                ],
                "play": {
                    "verb": "play",
                    "resumePosition": null,
                    "season": { "id": season_id(1), "number": 1 },
                    "episode": {
                        "id": first_episode,
                        "number": 1,
                        "title": "Good News About Hell",
                        "mediaFileId": null,
                    },
                },
                "moreLikeThis": null,
            })
        );
    }

    #[tokio::test]
    async fn the_show_page_resumes_the_signed_in_users_paused_episode() {
        let app = TestApp::new().await;
        let (noor, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let severance = add_show(&app, "Severance", &[1, 2]).await;
        let season_two = db_context
            .seasons()
            .filter(
                Season::fields()
                    .show_id()
                    .eq(severance)
                    .and(Season::fields().number().eq(2)),
            )
            .get(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let paused = db_context
            .episodes()
            .filter(Episode::fields().season_id().eq(season_two))
            .get(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let media_file = add_media_file(&app, None).await;
        db_context
            .episodes()
            .filter(Episode::fields().id().eq(paused))
            .update()
            .media_file_id(Some(media_file))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        stop_at(&app, noor, media_file, SignedDuration::from_secs(1534)).await;
        let server = serve(&app).await;

        let (status, page) = server
            .request(
                Method::GET,
                &format!("/api/shows/{severance}/v1"),
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            page["play"],
            json!({
                "verb": "resume",
                "resumePosition": 1534.0,
                "season": { "id": season_two, "number": 2 },
                "episode": {
                    "id": paused,
                    "number": 1,
                    "title": "Good News About Hell",
                    "mediaFileId": media_file,
                },
            })
        );
    }

    #[tokio::test]
    async fn an_unknown_out_of_library_or_malformed_show_id_returns_the_not_found_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let out_of_library = add_show(&app, "Severance", &[]).await;
        let server = serve(&app).await;

        for id in [
            out_of_library.to_string(),
            Uuid::new_v4().to_string(),
            "severance".to_owned(),
        ] {
            let (status, problem) = server
                .request(
                    Method::GET,
                    &format!("/api/shows/{id}/v1"),
                    Some(&token),
                    None,
                )
                .await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(problem["type"], "urn:selfless:problem:not-found", "{id}");
        }
    }
}
