use crate::routes::api::tile::TileBody;
use crate::user_auth::CurrentSession;
use application::browsing::get_home::FeaturedTitle;
use application::browsing::get_home::Row;
use application::browsing::get_home::RowKind;
use infrastructure::AppState;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::content::Json;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

#[derive(Serialize)]
struct HomeBody {
    featured: Option<FeaturedTitleBody>,
    rows: Vec<RowBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FeaturedTitleBody {
    tile: TileBody,
    play_episode_id: Option<Uuid>,
}

#[derive(Serialize)]
struct RowBody {
    kind: Kind,
    genre: Option<String>,
    tiles: Vec<TileBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    ContinueWatching,
    RecentlyAdded,
    PopularNow,
    Genre,
}

impl From<FeaturedTitle> for FeaturedTitleBody {
    fn from(featured: FeaturedTitle) -> Self {
        Self {
            tile: featured.tile.into(),
            play_episode_id: featured.play_episode_id,
        }
    }
}

impl From<Row> for RowBody {
    fn from(row: Row) -> Self {
        let (kind, genre) = match row.kind {
            RowKind::ContinueWatching => (Kind::ContinueWatching, None),
            RowKind::RecentlyAdded => (Kind::RecentlyAdded, None),
            RowKind::PopularNow => (Kind::PopularNow, None),
            RowKind::Genre(name) => (Kind::Genre, Some(name)),
        };
        Self {
            kind,
            genre,
            tiles: row.tiles.into_iter().map(TileBody::from).collect(),
        }
    }
}

#[route(GET "/api/(user)/home/v1")]
pub(crate) async fn home(cx: &Cx) -> topcoat::Result<Response> {
    let home = application::browsing::get_home::call(
        app_context::<AppState>(cx),
        request_context::<CurrentSession>(cx).user.id,
    )
    .await?;
    Json(&HomeBody {
        featured: home.featured.map(FeaturedTitleBody::from),
        rows: home.rows.into_iter().map(RowBody::from).collect(),
    })
    .into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::add_artwork;
    use crate::test::add_media_file;
    use crate::test::add_show;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use crate::test::stop_at;
    use application::ports::persistence::DbScope;
    use domain::ArtworkKind;
    use domain::Episode;
    use http::Method;
    use http::StatusCode;
    use jiff::SignedDuration;
    use jiff::civil::date;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;

    #[tokio::test]
    async fn home_returns_the_featured_title_and_the_rows_in_order() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let arrival = movie(&app, "Arrival")
            .added_at(Some(app.clock.now()))
            .premiered(Some(date(2016, 11, 11)))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        add_artwork(
            &app,
            arrival,
            ArtworkKind::Fanart,
            "/media/Movies/Arrival/fanart.jpg",
        )
        .await;
        let server = serve(&app).await;

        let (status, home) = server
            .request(Method::GET, "/api/home/v1", Some(&token), None)
            .await;

        let ids = |row: &Value| {
            row["tiles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["id"].clone())
                .collect::<Vec<_>>()
        };
        let rows: Vec<_> = home["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["kind"].clone(), row["genre"].clone(), ids(row)))
            .collect();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(home["featured"]["tile"]["id"], json!(arrival));
        assert_eq!(home["featured"]["playEpisodeId"], Value::Null);
        assert_eq!(
            home["featured"]["tile"]["images"]["fanart"]["urls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["width"].clone())
                .collect::<Vec<_>>(),
            [480, 960, 1280, 1920]
        );
        assert_eq!(
            rows,
            [
                (json!("recentlyAdded"), Value::Null, vec![json!(arrival)]),
                (json!("genre"), json!("Drama"), vec![json!(arrival)]),
                (
                    json!("genre"),
                    json!("Science Fiction"),
                    vec![json!(arrival)]
                ),
            ]
        );
    }

    #[tokio::test]
    async fn home_returns_an_episode_tile_in_continue_watching_and_the_show_in_popular_now() {
        let app = TestApp::new().await;
        let (noor, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let severance = add_show(&app, "Severance", &[2]).await;
        let media_file_id = add_media_file(&app, Some(3_000_000)).await;
        let episode = db_context
            .episodes()
            .all()
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        db_context
            .episodes()
            .filter(Episode::fields().id().eq(episode.id))
            .update()
            .media_file_id(Some(media_file_id))
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
        stop_at(&app, noor, media_file_id, SignedDuration::from_mins(15)).await;
        let server = serve(&app).await;

        let (status, home) = server
            .request(Method::GET, "/api/home/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(home["featured"]["tile"]["id"], json!(severance));
        assert_eq!(
            home["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["kind"].clone())
                .collect::<Vec<_>>(),
            [
                json!("continueWatching"),
                json!("popularNow"),
                json!("genre"),
                json!("genre")
            ]
        );
        let tile = &home["rows"][0]["tiles"][0];
        assert_eq!(
            (
                &tile["id"],
                &tile["kind"],
                &tile["runtimeMinutes"],
                &tile["progress"]
            ),
            (
                &json!(episode.id),
                &json!("episode"),
                &json!(50),
                &json!(0.3)
            )
        );
        assert_eq!(
            (
                &tile["episode"]["show"]["id"],
                &tile["episode"]["show"]["title"],
                &tile["episode"]["show"]["logo"]["urls"][0]["width"],
                &tile["episode"]["seasonNumber"],
                &tile["episode"]["episodeNumber"],
            ),
            (
                &json!(severance),
                &json!("Severance"),
                &json!(300),
                &json!(2),
                &json!(1)
            )
        );
        assert_eq!(home["rows"][1]["tiles"][0]["id"], json!(severance));
        assert_eq!(home["featured"]["playEpisodeId"], json!(episode.id));
    }

    #[tokio::test]
    async fn an_empty_library_has_no_featured_title_and_no_rows() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let server = serve(&app).await;

        let (status, home) = server
            .request(Method::GET, "/api/home/v1", Some(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(home, json!({ "featured": null, "rows": [] }));
    }

    #[tokio::test]
    async fn home_needs_a_signed_in_user() {
        let app = TestApp::new().await;
        let server = serve(&app).await;

        let (status, problem) = server.get("/api/home/v1").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(problem["type"], "urn:selfless:problem:unauthenticated");
    }
}
