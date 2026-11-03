use crate::problem::NOT_FOUND;
use crate::routes::api::image::ImageBody;
use application::browsing::list_season_episodes;
use infrastructure::AppState;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::content::Json;
use topcoat::router::path_param;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use uuid::Uuid;

path_param!(season_id: Uuid);

#[route(GET "/api/(user)/seasons/{season_id}/episodes/v1")]
pub(crate) async fn episodes(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(season_id) = path_param::<SeasonId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    match list_season_episodes::call(app_context::<AppState>(cx), *season_id).await? {
        list_season_episodes::Output::Episodes(episodes) => Json(&EpisodeListBody {
            episodes: episodes.into_iter().map(EpisodeBody::from).collect(),
        })
        .into_response(cx),
        list_season_episodes::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[derive(Serialize)]
struct EpisodeListBody {
    episodes: Vec<EpisodeBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeBody {
    id: Uuid,
    number: i64,
    title: String,
    runtime_minutes: Option<i64>,
    plot: Option<String>,
    thumb: Option<ImageBody>,
    progress: Option<f64>,
    watched: bool,
}

impl From<list_season_episodes::Episode> for EpisodeBody {
    fn from(episode: list_season_episodes::Episode) -> Self {
        Self {
            id: episode.id,
            number: episode.number,
            title: episode.title,
            runtime_minutes: episode.runtime_minutes,
            plot: episode.plot,
            thumb: episode.thumb.map(ImageBody::from),
            progress: None,
            watched: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test::add_show;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use domain::ArtworkKind;
    use domain::Season;
    use http::Method;
    use http::StatusCode;
    use serde_json::json;
    use test_app::TestApp;
    use uuid::Uuid;

    #[tokio::test]
    async fn a_season_returns_its_visible_episodes_with_runtime_plot_and_thumb() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let severance = add_show(&app, "Severance", &[1]).await;
        let season_id = db_context
            .seasons()
            .filter(Season::fields().show_id().eq(severance))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .id;
        let media_file = db_context
            .media_files()
            .create()
            .path("/media/TV/Severance/Season 01/Severance - S01E02.mkv")
            .duration_ms(Some(3_420_000))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let half_loop = db_context
            .episodes()
            .create()
            .season_id(season_id)
            .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .number(2)
            .title("Half Loop")
            .overview(Some("Mark takes Helly to the break room.".to_owned()))
            .media_file_id(Some(media_file.id))
            .visible(true)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        db_context
            .artwork()
            .create()
            .show_id(Some(severance))
            .episode_id(Some(half_loop.id))
            .kind(ArtworkKind::Thumb)
            .path("/media/TV/Severance/Season 01/Severance - S01E02-thumb.jpg")
            .modified_at(app.clock.now())
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        db_context
            .episodes()
            .create()
            .season_id(season_id)
            .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .number(3)
            .title("In Perpetuity")
            .visible(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        let (status, mut list) = server
            .request(
                Method::GET,
                &format!("/api/seasons/{season_id}/episodes/v1"),
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        let thumb = list["episodes"][1]["thumb"].take();
        assert_eq!(
            thumb["urls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["width"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [480, 960]
        );
        let first_id = list["episodes"][0]["id"].clone();
        assert_eq!(
            list,
            json!({
                "episodes": [
                    {
                        "id": first_id,
                        "number": 1,
                        "title": "Good News About Hell",
                        "runtimeMinutes": null,
                        "plot": null,
                        "thumb": null,
                        "progress": null,
                        "watched": false,
                    },
                    {
                        "id": half_loop.id,
                        "number": 2,
                        "title": "Half Loop",
                        "runtimeMinutes": 57,
                        "plot": "Mark takes Helly to the break room.",
                        "thumb": null,
                        "progress": null,
                        "watched": false,
                    },
                ],
            })
        );
    }

    #[tokio::test]
    async fn an_unknown_hidden_or_malformed_season_id_returns_the_not_found_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let mut db_context = app.state.db_context.clone();
        let severance = add_show(&app, "Severance", &[]).await;
        let empty = db_context
            .seasons()
            .create()
            .show_id(severance)
            .number(3)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        for id in [
            empty.id.to_string(),
            Uuid::new_v4().to_string(),
            "season-1".to_owned(),
        ] {
            let (status, problem) = server
                .request(
                    Method::GET,
                    &format!("/api/seasons/{id}/episodes/v1"),
                    Some(&token),
                    None,
                )
                .await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(problem["type"], "urn:selfless:problem:not-found", "{id}");
        }
    }
}
