use crate::problem::INVALID_SIGNATURE;
use crate::problem::NEEDS_HLS;
use crate::problem::NOT_FOUND;
use crate::problem::PLAYBACK_ENDED;
use crate::problem::UNREADABLE_MEDIA_FILE;
use crate::user_auth::CurrentSession;
use application::playback::get_playback_file;
use application::playback::report_progress;
use application::playback::start_playback;
use domain::AudioCodecSupport;
use domain::CapabilityProfile;
use domain::HdrType;
use domain::PlayMethod;
use domain::PlayReason;
use domain::VideoCodecSupport;
use infrastructure::AppState;
use serde::Deserialize;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::context::request_context;
use topcoat::router::Body;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::path_param;
use topcoat::router::request::headers;
use topcoat::router::request::method;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;
use tower_http::services::ServeFile;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPlayback {
    media_file_id: Uuid,
    start_position: f64,
    profile: ProfileBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBody {
    containers: Vec<String>,
    video_codecs: Vec<VideoCodecBody>,
    audio_codecs: Vec<AudioCodecBody>,
    subtitle_codecs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoCodecBody {
    codec: String,
    profiles: Vec<String>,
    max_width: i64,
    max_height: i64,
    hdr_types: Vec<HdrTypeBody>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioCodecBody {
    codec: String,
    max_channels: i64,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum HdrTypeBody {
    Sdr,
    Hdr10,
    Hlg,
    DolbyVision,
}

impl From<HdrTypeBody> for HdrType {
    fn from(hdr_type: HdrTypeBody) -> Self {
        match hdr_type {
            HdrTypeBody::Sdr => Self::Sdr,
            HdrTypeBody::Hdr10 => Self::Hdr10,
            HdrTypeBody::Hlg => Self::Hlg,
            HdrTypeBody::DolbyVision => Self::DolbyVision,
        }
    }
}

impl From<ProfileBody> for CapabilityProfile {
    fn from(profile: ProfileBody) -> Self {
        Self {
            containers: profile.containers,
            video_codecs: profile
                .video_codecs
                .into_iter()
                .map(|x| VideoCodecSupport {
                    codec: x.codec,
                    profiles: x.profiles,
                    max_width: x.max_width,
                    max_height: x.max_height,
                    hdr_types: x.hdr_types.into_iter().map(HdrType::from).collect(),
                })
                .collect(),
            audio_codecs: profile
                .audio_codecs
                .into_iter()
                .map(|x| AudioCodecSupport {
                    codec: x.codec,
                    max_channels: x.max_channels,
                })
                .collect(),
            subtitle_codecs: profile.subtitle_codecs,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackBody {
    playback_id: Uuid,
    method: MethodBody,
    reasons: Vec<ReasonBody>,
    url: String,
    duration: Option<f64>,
    movie: Option<MovieBody>,
    episodes: Vec<EpisodeBody>,
    audio_tracks: Vec<AudioTrackBody>,
    subtitle_tracks: Vec<SubtitleTrackBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum MethodBody {
    DirectPlay,
    Remux,
    Transcode,
}

#[derive(Serialize)]
struct ReasonBody {
    kind: &'static str,
    value: String,
}

#[derive(Serialize)]
struct MovieBody {
    id: Uuid,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeBody {
    id: Uuid,
    show_title: String,
    season_number: i64,
    episode_number: i64,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioTrackBody {
    id: Uuid,
    language: Option<String>,
    label: Option<String>,
    codec: String,
    channels: i64,
    default: bool,
    in_stream: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleTrackBody {
    id: Uuid,
    language: Option<String>,
    label: Option<String>,
    codec: String,
    default: bool,
    forced: bool,
    hearing_impaired: bool,
    external: bool,
    text: bool,
    in_stream: bool,
}

impl From<PlayReason> for ReasonBody {
    fn from(reason: PlayReason) -> Self {
        let (kind, value) = match reason {
            PlayReason::Container(x) => ("container", x),
            PlayReason::VideoCodec(x) => ("videoCodec", x),
            PlayReason::VideoProfile(x) => ("videoProfile", x),
            PlayReason::VideoSize { width, height } => ("videoSize", format!("{width}x{height}")),
            PlayReason::VideoRange(x) => (
                "videoRange",
                match x {
                    HdrType::Sdr => "sdr",
                    HdrType::Hdr10 => "hdr10",
                    HdrType::Hlg => "hlg",
                    HdrType::DolbyVision => "dolbyVision",
                }
                .to_owned(),
            ),
            PlayReason::AudioCodec(x) => ("audioCodec", x),
            PlayReason::AudioChannels(x) => ("audioChannels", x.to_string()),
        };
        Self { kind, value }
    }
}

impl From<start_playback::Playback> for PlaybackBody {
    fn from(playback: start_playback::Playback) -> Self {
        Self {
            playback_id: playback.id,
            method: match playback.method {
                PlayMethod::DirectPlay => MethodBody::DirectPlay,
                PlayMethod::Remux => MethodBody::Remux,
                PlayMethod::Transcode => MethodBody::Transcode,
            },
            reasons: playback.reasons.into_iter().map(ReasonBody::from).collect(),
            url: format!(
                "/api/playbacks/{}/media/{}/v1/file",
                playback.id, playback.signature
            ),
            duration: playback.duration.map(|x| x.as_secs_f64()),
            movie: playback.movie.map(|x| MovieBody {
                id: x.id,
                title: x.title,
            }),
            episodes: playback
                .episodes
                .into_iter()
                .map(|x| EpisodeBody {
                    id: x.id,
                    show_title: x.show_title,
                    season_number: x.season_number,
                    episode_number: x.episode_number,
                    title: x.title,
                })
                .collect(),
            audio_tracks: playback
                .audio_tracks
                .into_iter()
                .map(|x| AudioTrackBody {
                    id: x.id,
                    language: x.language,
                    label: x.label,
                    codec: x.codec,
                    channels: x.channels,
                    default: x.default,
                    in_stream: x.in_stream,
                })
                .collect(),
            subtitle_tracks: playback
                .subtitle_tracks
                .into_iter()
                .map(|x| SubtitleTrackBody {
                    id: x.id,
                    language: x.language,
                    label: x.label,
                    codec: x.codec,
                    default: x.default,
                    forced: x.forced,
                    hearing_impaired: x.hearing_impaired,
                    external: x.external,
                    text: x.text,
                    in_stream: x.in_stream,
                })
                .collect(),
        }
    }
}

#[route(POST "/api/(user)/playbacks/v1")]
pub(crate) async fn start(
    cx: &Cx,
    Json(request): Json<StartPlayback>,
) -> topcoat::Result<Response> {
    let session = request_context::<CurrentSession>(cx);
    match start_playback::call(
        app_context::<AppState>(cx),
        session.user.id,
        session.id,
        session.client_kind,
        request.media_file_id,
        request.start_position,
        &request.profile.into(),
    )
    .await?
    {
        start_playback::Output::Playback(playback) => {
            (StatusCode::CREATED, Json(PlaybackBody::from(*playback))).into_response(cx)
        }
        start_playback::Output::NotFound => NOT_FOUND.into_response(cx),
        start_playback::Output::UnreadableMediaFile => UNREADABLE_MEDIA_FILE.into_response(cx),
        start_playback::Output::NeedsHls => NEEDS_HLS.into_response(cx),
    }
}

#[derive(Deserialize)]
struct ProgressReport {
    position: f64,
    paused: bool,
    stopped: bool,
}

path_param!(playback_id: Uuid);
path_param!(signature: String);

#[route(PUT "/api/(user)/playbacks/{playback_id}/progress/v1")]
pub(crate) async fn progress(
    cx: &Cx,
    Json(request): Json<ProgressReport>,
) -> topcoat::Result<Response> {
    let Ok(playback_id) = path_param::<PlaybackId>(cx) else {
        return NOT_FOUND.into_response(cx);
    };
    match report_progress::call(
        app_context::<AppState>(cx),
        request_context::<CurrentSession>(cx).user.id,
        *playback_id,
        request.position,
        request.paused,
        request.stopped,
    )
    .await?
    {
        report_progress::Output::Reported => StatusCode::NO_CONTENT.into_response(cx),
        report_progress::Output::NotFound => NOT_FOUND.into_response(cx),
        report_progress::Output::Ended => PLAYBACK_ENDED.into_response(cx),
    }
}

#[route(GET "/api/(public)/playbacks/{playback_id}/media/{signature}/v1/file")]
pub(crate) async fn file(cx: &Cx) -> topcoat::Result<Response> {
    let (Ok(playback_id), Ok(signature)) =
        (path_param::<PlaybackId>(cx), path_param::<Signature>(cx))
    else {
        return NOT_FOUND.into_response(cx);
    };
    let path = match get_playback_file::call(app_context::<AppState>(cx), *playback_id, signature)
        .await?
    {
        get_playback_file::Output::File { path } => path,
        get_playback_file::Output::NotFound => return NOT_FOUND.into_response(cx),
        get_playback_file::Output::InvalidSignature => {
            return INVALID_SIGNATURE.into_response(cx);
        }
    };
    let mut request = http::Request::new(());
    *request.method_mut() = method(cx).clone();
    *request.headers_mut() = headers(cx).clone();
    Ok(ServeFile::new(path).try_call(request).await?.map(Body::new))
}

#[cfg(test)]
mod tests {
    use crate::test::add_show;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::persistence::DbScope;
    use domain::Episode;
    use domain::HdrType;
    use domain::Season;
    use http::HeaderMap;
    use http::HeaderValue;
    use http::Method;
    use http::StatusCode;
    use http::header::CONTENT_RANGE;
    use http::header::CONTENT_TYPE;
    use http::header::RANGE;
    use opentelemetry::Array;
    use opentelemetry::Value as OtelValue;
    use opentelemetry::trace::SpanKind;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use opentelemetry_sdk::trace::SpanData;
    use serde_json::Value;
    use serde_json::json;
    use std::path::PathBuf;
    use test_app::TestApp;
    use test_fixtures::TestServer;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use uuid::Uuid;

    const NAME: &str = "Noor";
    const PASSWORD: &str = "a long passphrase about tea";
    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";
    const BYTES: &[u8] = b"0123456789";

    struct MediaFileOnDisk(PathBuf);

    impl Drop for MediaFileOnDisk {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    async fn add_media_file(
        app: &TestApp,
        extension: &str,
        container: Option<&str>,
    ) -> (Uuid, MediaFileOnDisk) {
        let path = std::env::temp_dir().join(format!("selfless-{}.{extension}", Uuid::new_v4()));
        std::fs::write(&path, BYTES).unwrap();
        let mut db_context = app.state.db_context.clone();
        let media_file = db_context
            .media_files()
            .create()
            .path(path.to_str().unwrap())
            .container(container.map(str::to_owned))
            .duration_ms(Some(6_981_000))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        media_file
            .video_streams()
            .create()
            .index(0)
            .codec("h264")
            .profile(Some("High".to_owned()))
            .width(1920)
            .height(800)
            .hdr_type(HdrType::Sdr)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (index, language, title, default) in
            [(2, "jpn", "Stereo", false), (1, "eng", "Stereo", true)]
        {
            media_file
                .audio_streams()
                .create()
                .index(index)
                .codec("aac")
                .language(Some(language.to_owned()))
                .channels(2)
                .title(Some(title.to_owned()))
                .is_default(default)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        (media_file.id, MediaFileOnDisk(path))
    }

    async fn add_movie_playing(app: &TestApp, media_file_id: Uuid, in_library: bool) -> Uuid {
        let mut db_context = app.state.db_context.clone();
        movie(app, "Arrival")
            .media_file_id(Some(media_file_id))
            .in_library(in_library)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    fn browser_profile() -> Value {
        json!({
            "containers": ["mp4", "webm"],
            "videoCodecs": [{
                "codec": "h264",
                "profiles": ["main", "high"],
                "maxWidth": 3840,
                "maxHeight": 2160,
                "hdrTypes": ["sdr"],
            }],
            "audioCodecs": [{ "codec": "aac", "maxChannels": 2 }],
            "subtitleCodecs": [],
        })
    }

    async fn start(server: &TestServer, token: &str, media_file_id: Uuid) -> (StatusCode, Value) {
        server
            .request(
                Method::POST,
                "/api/playbacks/v1",
                Some(token),
                Some(json!({
                    "mediaFileId": media_file_id,
                    "startPosition": 0,
                    "profile": browser_profile(),
                })),
            )
            .await
    }

    async fn report(
        server: &TestServer,
        token: &str,
        playback: &Value,
        stopped: bool,
    ) -> (StatusCode, Value) {
        server
            .request(
                Method::PUT,
                &format!(
                    "/api/playbacks/{}/progress/v1",
                    playback["playbackId"].as_str().unwrap()
                ),
                Some(token),
                Some(json!({ "position": 12.5, "paused": false, "stopped": stopped })),
            )
            .await
    }

    #[tokio::test]
    async fn a_file_the_profile_covers_plays_directly_with_its_movie_and_tracks() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, file) = add_media_file(&app, "mp4", Some(MP4)).await;
        let movie_id = add_movie_playing(&app, media_file_id, true).await;
        let mut db_context = app.state.db_context.clone();
        let subtitles = db_context
            .media_files()
            .find(media_file_id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .subtitle_files()
            .create()
            .path(file.0.with_extension("en.srt").to_str().unwrap())
            .language(Some("en".to_owned()))
            .hearing_impaired(false)
            .forced(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let server = serve(&app).await;

        let (status, mut playback) = start(&server, &token, media_file_id).await;

        assert_eq!(status, StatusCode::CREATED);
        let playback_id = playback["playbackId"].as_str().unwrap().to_owned();
        let url = playback["url"].take();
        assert!(
            url.as_str()
                .unwrap()
                .starts_with(&format!("/api/playbacks/{playback_id}/media/")),
            "{url}"
        );
        assert!(url.as_str().unwrap().ends_with("/v1/file"), "{url}");
        for track in playback["audioTracks"].as_array_mut().unwrap() {
            track["id"].take();
        }
        playback["playbackId"].take();
        assert_eq!(
            playback,
            json!({
                "playbackId": null,
                "method": "directPlay",
                "reasons": [],
                "url": null,
                "duration": 6981.0,
                "movie": { "id": movie_id, "title": "Arrival" },
                "episodes": [],
                "audioTracks": [
                    {
                        "id": null,
                        "language": "eng",
                        "label": "Stereo",
                        "codec": "aac",
                        "channels": 2,
                        "default": true,
                        "inStream": true,
                    },
                    {
                        "id": null,
                        "language": "jpn",
                        "label": "Stereo",
                        "codec": "aac",
                        "channels": 2,
                        "default": false,
                        "inStream": true,
                    },
                ],
                "subtitleTracks": [{
                    "id": subtitles,
                    "language": "en",
                    "label": null,
                    "codec": "srt",
                    "default": false,
                    "forced": false,
                    "hearingImpaired": false,
                    "external": true,
                    "text": true,
                    "inStream": false,
                }],
            })
        );
    }

    #[tokio::test]
    async fn a_two_episode_file_plays_as_both_episodes_in_order() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        let show_id = add_show(&app, "Severance", &[1]).await;
        let mut db_context = app.state.db_context.clone();
        let season = db_context
            .seasons()
            .filter(Season::fields().show_id().eq(show_id))
            .get(db_context.executor_mut())
            .await
            .unwrap();
        let mut ids = Vec::new();
        for (number, title) in [(3, "In Perpetuity"), (2, "Half Loop")] {
            ids.push(
                Episode::create()
                    .season_id(season.id)
                    .tvdb_id(domain::TvdbId(number))
                    .number(number)
                    .title(title)
                    .media_file_id(Some(media_file_id))
                    .in_library(true)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap()
                    .id,
            );
        }
        let server = serve(&app).await;

        let (status, playback) = start(&server, &token, media_file_id).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["movie"], Value::Null);
        assert_eq!(
            playback["episodes"],
            json!([
                {
                    "id": ids[1],
                    "showTitle": "Severance",
                    "seasonNumber": 1,
                    "episodeNumber": 2,
                    "title": "Half Loop",
                },
                {
                    "id": ids[0],
                    "showTitle": "Severance",
                    "seasonNumber": 1,
                    "episodeNumber": 3,
                    "title": "In Perpetuity",
                },
            ])
        );
    }

    #[tokio::test]
    async fn a_file_that_needs_hls_returns_the_needs_hls_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, problem) = start(&server, &token, media_file_id).await;

        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(problem["type"], "urn:selfless:problem:needs-hls");
    }

    #[tokio::test]
    async fn the_play_request_span_carries_the_play_method_and_the_reason_kinds() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (mp4, _mp4_file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, mp4, true).await;
        let (mkv, _mkv_file) = add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, mkv, true).await;
        let server = serve(&app).await;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );

        start(&server, &token, mp4).await;
        start(&server, &token, mkv).await;

        let attribute = |span: &SpanData, key: &str| {
            span.attributes
                .iter()
                .find(|x| x.key.as_str() == key)
                .map(|x| x.value.clone())
        };
        let spans = exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| x.span_kind == SpanKind::Server)
            .map(|x| {
                (
                    attribute(&x, "selfless.playback.method"),
                    attribute(&x, "selfless.playback.reason_kinds"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            spans,
            [
                (Some(OtelValue::from("directPlay")), None),
                (
                    None,
                    Some(OtelValue::Array(Array::String(vec!["container".into()])))
                ),
            ]
        );
    }

    #[tokio::test]
    async fn an_unknown_or_hidden_media_file_returns_404() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (hidden, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, hidden, false).await;
        let server = serve(&app).await;

        for media_file_id in [hidden, Uuid::new_v4()] {
            let (status, problem) = start(&server, &token, media_file_id).await;

            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(problem["type"], "urn:selfless:problem:not-found");
        }
    }

    #[tokio::test]
    async fn a_media_file_whose_streams_sync_could_not_read_returns_422() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", None).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, problem) = start(&server, &token, media_file_id).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            problem["type"],
            "urn:selfless:problem:unreadable-media-file"
        );
    }

    #[tokio::test]
    async fn a_new_playback_ends_the_open_playback_of_the_same_session() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (_, other_token) = signed_in_user(&app, "Theo", PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, first) = start(&server, &token, media_file_id).await;
        let (_, other_session) = start(&server, &other_token, media_file_id).await;

        let (_, second) = start(&server, &token, media_file_id).await;

        assert_eq!(
            report(&server, &token, &first, false).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            report(&server, &token, &second, false).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            report(&server, &other_token, &other_session, false).await.0,
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn a_report_after_a_stop_returns_the_playback_ended_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, playback) = start(&server, &token, media_file_id).await;

        let (progress, _) = report(&server, &token, &playback, false).await;
        let (stop, _) = report(&server, &token, &playback, true).await;
        let (after_stop, problem) = report(&server, &token, &playback, false).await;

        assert_eq!(progress, StatusCode::NO_CONTENT);
        assert_eq!(stop, StatusCode::NO_CONTENT);
        assert_eq!(after_stop, StatusCode::NOT_FOUND);
        assert_eq!(problem["type"], "urn:selfless:problem:playback-ended");
    }

    #[tokio::test]
    async fn the_file_url_of_an_ended_playback_returns_404() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, playback) = start(&server, &token, media_file_id).await;
        report(&server, &token, &playback, true).await;

        let (status, _, problem) = server.get_bytes(playback["url"].as_str().unwrap()).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        let problem = serde_json::from_slice::<Value>(&problem).unwrap();
        assert_eq!(problem["type"], "urn:selfless:problem:not-found");
    }

    #[tokio::test]
    async fn a_report_stores_the_position_paused_state_and_report_time() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, playback) = start(&server, &token, media_file_id).await;
        app.clock.advance(jiff::SignedDuration::from_secs(10));

        report(&server, &token, &playback, false).await;

        let mut db_context = app.state.db_context.clone();
        let stored = db_context
            .playbacks()
            .find(
                playback["playbackId"]
                    .as_str()
                    .unwrap()
                    .parse::<Uuid>()
                    .unwrap(),
            )
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (stored.position, stored.paused, stored.last_reported_at),
            (12.5, false, app.clock.now())
        );
    }

    #[tokio::test]
    async fn another_users_report_returns_404() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (_, other_token) = signed_in_user(&app, "Theo", PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, playback) = start(&server, &token, media_file_id).await;

        let (status, problem) = report(&server, &other_token, &playback, false).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(problem["type"], "urn:selfless:problem:not-found");
    }

    #[tokio::test]
    async fn the_file_url_serves_the_media_file_with_range_requests_and_no_session_token() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, playback) = start(&server, &token, media_file_id).await;
        let url = playback["url"].as_str().unwrap();
        let mut range = HeaderMap::new();
        range.insert(RANGE, HeaderValue::from_static("bytes=2-5"));

        let (whole_status, whole_headers, whole) = server.get_bytes(url).await;
        let (part_status, part_headers, part) = server.get_bytes_with_headers(url, range).await;

        assert_eq!(whole_status, StatusCode::OK);
        assert_eq!(whole_headers[CONTENT_TYPE], "video/mp4");
        assert_eq!(whole, BYTES);
        assert_eq!(part_status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(part_headers[CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(part, b"2345");
    }

    #[tokio::test]
    async fn a_bad_signature_returns_the_invalid_signature_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, first) = start(&server, &token, media_file_id).await;
        let (_, second) = start(&server, &token, media_file_id).await;
        let first_id = first["playbackId"].as_str().unwrap();
        let second_id = second["playbackId"].as_str().unwrap();

        for url in [
            first["url"].as_str().unwrap().replace(first_id, second_id),
            format!("/api/playbacks/{first_id}/media/not-the-signature/v1/file"),
        ] {
            let (status, _, problem) = server.get_bytes(&url).await;

            assert_eq!(status, StatusCode::FORBIDDEN, "{url}");
            let problem = serde_json::from_slice::<Value>(&problem).unwrap();
            assert_eq!(problem["type"], "urn:selfless:problem:invalid-signature");
        }
    }
}
