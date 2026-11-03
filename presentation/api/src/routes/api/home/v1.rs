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
    RecentlyAdded,
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
            RowKind::RecentlyAdded => (Kind::RecentlyAdded, None),
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
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use domain::ArtworkKind;
    use http::Method;
    use http::StatusCode;
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
