use crate::problem::NOT_FOUND;
use crate::routes::api::image::ImageBody;
use application::browsing::get_episode;
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

path_param!(episode_id: Uuid);

#[route(GET "/api/(user)/episodes/{episode_id}/v1")]
pub(crate) async fn episode(cx: &Cx) -> topcoat::Result<Response> {
    let Ok(episode_id) = path_param::<EpisodeId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    match get_episode::call(app_context::<AppState>(cx), *episode_id).await? {
        get_episode::Output::Episode(page) => Json(&EpisodePageBody::from(*page)).into_response(cx),
        get_episode::Output::NotFound => NOT_FOUND.into_response(cx),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpisodePageBody {
    id: Uuid,
    title: String,
    season_number: i64,
    episode_number: i64,
    air_date: Option<String>,
    runtime_minutes: Option<i64>,
    rating: Option<f64>,
    plot: Option<String>,
    thumb: Option<ImageBody>,
    show: ShowBody,
    audio_languages: Vec<String>,
    subtitle_languages: Vec<SubtitleLanguageBody>,
    progress: Option<f64>,
    watched: bool,
    play: EpisodePlayBody,
}

#[derive(Serialize)]
struct ShowBody {
    id: Uuid,
    title: String,
    logo: Option<ImageBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleLanguageBody {
    language: String,
    hearing_impaired: bool,
    forced: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpisodePlayBody {
    resume_position: Option<f64>,
}

impl From<get_episode::Episode> for EpisodePageBody {
    fn from(page: get_episode::Episode) -> Self {
        Self {
            id: page.id,
            title: page.title,
            season_number: page.season_number,
            episode_number: page.episode_number,
            air_date: page.air_date.map(|x| x.to_string()),
            runtime_minutes: page.runtime_minutes,
            rating: page.rating,
            plot: page.plot,
            thumb: page.thumb.map(ImageBody::from),
            show: ShowBody {
                id: page.show.id,
                title: page.show.title,
                logo: page.show.logo.map(ImageBody::from),
            },
            audio_languages: page.audio_languages,
            subtitle_languages: page
                .subtitle_languages
                .into_iter()
                .map(|x| SubtitleLanguageBody {
                    language: x.language,
                    hearing_impaired: x.hearing_impaired,
                    forced: x.forced,
                })
                .collect(),
            progress: None,
            watched: false,
            play: EpisodePlayBody {
                resume_position: None,
            },
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
    use domain::Episode;
    use domain::Season;
    use http::Method;
    use http::StatusCode;
    use jiff::civil::date;
    use serde_json::Value;
    use serde_json::json;
    use test_app::TestApp;
    use uuid::Uuid;

    fn widths(image: &Value) -> Vec<i64> {
        image["urls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["width"].as_i64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn an_episode_returns_its_details_show_and_languages() {
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
        let half_loop = Episode::create()
            .season_id(season_id)
            .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .number(2)
            .title("Half Loop")
            .air_date(Some(date(2022, 2, 18)))
            .rating(Some(8.1))
            .overview(Some("Mark takes Helly to the break room.".to_owned()))
            .media_file_id(Some(media_file.id))
            .visible(true)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (episode_id, kind) in [
            (Some(half_loop.id), ArtworkKind::Thumb),
            (None, ArtworkKind::Clearlogo),
        ] {
            db_context
                .artwork()
                .create()
                .show_id(Some(severance))
                .episode_id(episode_id)
                .kind(kind)
                .path(format!("/media/TV/Severance/{kind:?}"))
                .modified_at(app.clock.now())
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        db_context
            .audio_streams()
            .create()
            .media_file_id(media_file.id)
            .index(1)
            .codec("eac3")
            .language(Some("eng".to_owned()))
            .channels(6)
            .is_default(true)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (index, language, text) in [(2, "eng", true), (3, "deu", false)] {
            db_context
                .subtitle_streams()
                .create()
                .media_file_id(media_file.id)
                .index(index)
                .codec(if text { "subrip" } else { "hdmv_pgs_subtitle" })
                .language(Some(language.to_owned()))
                .is_default(false)
                .forced(true)
                .hearing_impaired(false)
                .text(text)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        db_context
            .subtitle_files()
            .create()
            .media_file_id(media_file.id)
            .path("/media/TV/Severance/Season 01/Severance - S01E02.en.hi.srt")
            .language(Some("en".to_owned()))
            .hearing_impaired(true)
            .forced(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        let (status, mut page) = server
            .request(
                Method::GET,
                &format!("/api/episodes/{}/v1", half_loop.id),
                Some(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(widths(&page["thumb"].take()), [480, 960]);
        assert_eq!(widths(&page["show"]["logo"].take()), [300, 600]);
        assert_eq!(
            page,
            json!({
                "id": half_loop.id,
                "title": "Half Loop",
                "seasonNumber": 1,
                "episodeNumber": 2,
                "airDate": "2022-02-18",
                "runtimeMinutes": 57,
                "rating": 8.1,
                "plot": "Mark takes Helly to the break room.",
                "thumb": null,
                "show": { "id": severance, "title": "Severance", "logo": null },
                "audioLanguages": ["eng"],
                "subtitleLanguages": [
                    { "language": "eng", "hearingImpaired": false, "forced": true },
                    { "language": "en", "hearingImpaired": true, "forced": false },
                ],
                "progress": null,
                "watched": false,
                "play": { "resumePosition": null },
            })
        );
    }

    #[tokio::test]
    async fn an_unknown_hidden_or_malformed_episode_id_returns_the_not_found_problem() {
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
        let hidden = Episode::create()
            .season_id(season_id)
            .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .number(2)
            .title("Half Loop")
            .visible(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        for id in [
            hidden.id.to_string(),
            Uuid::new_v4().to_string(),
            "episode-2".to_owned(),
        ] {
            let (status, problem) = server
                .request(
                    Method::GET,
                    &format!("/api/episodes/{id}/v1"),
                    Some(&token),
                    None,
                )
                .await;

            assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(problem["type"], "urn:selfless:problem:not-found", "{id}");
        }
    }
}
