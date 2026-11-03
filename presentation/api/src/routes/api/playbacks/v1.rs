use crate::problem::INVALID_SIGNATURE;
use crate::problem::NEEDS_TRANSCODE;
use crate::problem::NOT_FOUND;
use crate::problem::PLAYBACK_ENDED;
use crate::problem::UNREADABLE_MEDIA_FILE;
use crate::user_auth::CurrentSession;
use application::playback::get_hls_file;
use application::playback::get_playback_file;
use application::playback::report_progress;
use application::playback::start_playback;
use domain::AudioCodecSupport;
use domain::CapabilityProfile;
use domain::HdrType;
use domain::PlayMethod;
use domain::PlayReason;
use domain::VideoCodecSupport;
use http::HeaderValue;
use http::header::CONTENT_TYPE;
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
    #[serde(default)]
    force_hls: bool,
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
    value: Option<String>,
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
            PlayReason::Container(x) => ("container", Some(x)),
            PlayReason::VideoCodec(x) => ("videoCodec", Some(x)),
            PlayReason::VideoProfile(x) => ("videoProfile", Some(x)),
            PlayReason::VideoSize { width, height } => {
                ("videoSize", Some(format!("{width}x{height}")))
            }
            PlayReason::VideoRange(x) => (
                "videoRange",
                Some(
                    match x {
                        HdrType::Sdr => "sdr",
                        HdrType::Hdr10 => "hdr10",
                        HdrType::Hlg => "hlg",
                        HdrType::DolbyVision => "dolbyVision",
                    }
                    .to_owned(),
                ),
            ),
            PlayReason::AudioCodec(x) => ("audioCodec", Some(x)),
            PlayReason::AudioChannels(x) => ("audioChannels", Some(x.to_string())),
            PlayReason::NoKeyframeIndex => ("noKeyframeIndex", None),
            PlayReason::ForcedHls => ("forcedHls", None),
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
                "/api/playbacks/{}/media/{}/v1/{}",
                playback.id,
                playback.signature,
                match playback.method {
                    PlayMethod::DirectPlay => "file",
                    PlayMethod::Remux | PlayMethod::Transcode => "main.m3u8",
                }
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
        request.force_hls,
    )
    .await?
    {
        start_playback::Output::Playback(playback) => {
            (StatusCode::CREATED, Json(PlaybackBody::from(*playback))).into_response(cx)
        }
        start_playback::Output::NotFound => NOT_FOUND.into_response(cx),
        start_playback::Output::UnreadableMediaFile => UNREADABLE_MEDIA_FILE.into_response(cx),
        start_playback::Output::NeedsTranscode => NEEDS_TRANSCODE.into_response(cx),
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
path_param!(file_name: String);

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

#[route(GET "/api/(public)/playbacks/{playback_id}/media/{signature}/v1/{file_name}")]
pub(crate) async fn hls_file(cx: &Cx) -> topcoat::Result<Response> {
    let (Ok(playback_id), Ok(signature), Ok(file_name)) = (
        path_param::<PlaybackId>(cx),
        path_param::<Signature>(cx),
        path_param::<FileName>(cx),
    ) else {
        return NOT_FOUND.into_response(cx);
    };
    let path = match get_hls_file::call(
        app_context::<AppState>(cx),
        *playback_id,
        signature,
        file_name,
    )
    .await?
    {
        get_hls_file::Output::File { path } => path,
        get_hls_file::Output::NotFound => return NOT_FOUND.into_response(cx),
        get_hls_file::Output::InvalidSignature => {
            return INVALID_SIGNATURE.into_response(cx);
        }
    };
    let content_type = match file_name.rsplit_once('.').map(|(_, x)| x) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("m4s") => "video/iso.segment",
        _ => "video/mp4",
    };
    let mut request = http::Request::new(());
    *request.method_mut() = method(cx).clone();
    *request.headers_mut() = headers(cx).clone();
    let mut response = ServeFile::new(path).try_call(request).await?;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response.map(Body::new))
}

#[cfg(test)]
mod tests {
    use crate::test::add_show;
    use crate::test::movie;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::ReadKeyframes;
    use application::ports::persistence::DbScope;
    use domain::AudioTranscode;
    use domain::AudioTranscodeCodec;
    use domain::Episode;
    use domain::HdrType;
    use domain::MediaFile;
    use domain::Season;
    use domain::VideoStream;
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
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use test_app::MAIN_PLAYLIST;
    use test_app::SEGMENT;
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
        add_media_file_with_audio(
            app,
            extension,
            container,
            &[
                (2, "aac", 2, "jpn", "Stereo", false),
                (1, "aac", 2, "eng", "Stereo", true),
            ],
        )
        .await
    }

    async fn add_media_file_with_audio(
        app: &TestApp,
        extension: &str,
        container: Option<&str>,
        audio: &[(i64, &str, i64, &str, &str, bool)],
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
        for &(index, codec, channels, language, title, default) in audio {
            media_file
                .audio_streams()
                .create()
                .index(index)
                .codec(codec)
                .language(Some(language.to_owned()))
                .channels(channels)
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

    fn apple_tv_profile() -> Value {
        json!({
            "containers": ["mp4", "mov"],
            "videoCodecs": [{
                "codec": "h264",
                "profiles": ["main", "high"],
                "maxWidth": 3840,
                "maxHeight": 2160,
                "hdrTypes": ["sdr"],
            }],
            "audioCodecs": [
                { "codec": "aac", "maxChannels": 2 },
                { "codec": "ac3", "maxChannels": 6 },
                { "codec": "eac3", "maxChannels": 8 },
            ],
            "subtitleCodecs": [],
        })
    }

    fn google_tv_profile_without_e_ac_3() -> Value {
        json!({
            "containers": ["mkv", "mp4", "webm"],
            "videoCodecs": [{
                "codec": "h264",
                "profiles": ["main", "high"],
                "maxWidth": 3840,
                "maxHeight": 2160,
                "hdrTypes": ["sdr"],
            }],
            "audioCodecs": [
                { "codec": "aac", "maxChannels": 8 },
                { "codec": "ac3", "maxChannels": 6 },
            ],
            "subtitleCodecs": [],
        })
    }

    async fn start(server: &TestServer, token: &str, media_file_id: Uuid) -> (StatusCode, Value) {
        start_on(server, token, media_file_id, browser_profile()).await
    }

    async fn start_on(
        server: &TestServer,
        token: &str,
        media_file_id: Uuid,
        profile: Value,
    ) -> (StatusCode, Value) {
        server
            .request(
                Method::POST,
                "/api/playbacks/v1",
                Some(token),
                Some(json!({
                    "mediaFileId": media_file_id,
                    "startPosition": 0,
                    "profile": profile,
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

    async fn start_with(
        server: &TestServer,
        token: &str,
        media_file_id: Uuid,
        force_hls: bool,
    ) -> (StatusCode, Value) {
        server
            .request(
                Method::POST,
                "/api/playbacks/v1",
                Some(token),
                Some(json!({
                    "mediaFileId": media_file_id,
                    "startPosition": 0,
                    "profile": browser_profile(),
                    "forceHls": force_hls,
                })),
            )
            .await
    }

    fn sibling(playback: &Value, file_name: &str) -> String {
        let url = playback["url"].as_str().unwrap();
        format!("{}/{file_name}", url.rsplit_once('/').unwrap().0)
    }

    fn playback_id(playback: &Value) -> Uuid {
        playback["playbackId"].as_str().unwrap().parse().unwrap()
    }

    #[tokio::test]
    async fn a_file_whose_container_alone_is_wrong_remuxes_to_hls_with_a_container_reason() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, playback) = start(&server, &token, media_file_id).await;
        let (main_status, main_headers, main) =
            server.get_bytes(playback["url"].as_str().unwrap()).await;
        let (segment_status, segment_headers, segment) =
            server.get_bytes(&sibling(&playback, "video-0.m4s")).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["method"], "remux");
        assert_eq!(
            playback["reasons"],
            json!([{ "kind": "container", "value": "mkv" }])
        );
        assert!(
            playback["url"].as_str().unwrap().ends_with("/v1/main.m3u8"),
            "{}",
            playback["url"]
        );
        assert_eq!(main_status, StatusCode::OK);
        assert_eq!(main_headers[CONTENT_TYPE], "application/vnd.apple.mpegurl");
        assert_eq!(main, MAIN_PLAYLIST.as_bytes());
        assert_eq!(segment_status, StatusCode::OK);
        assert_eq!(segment_headers[CONTENT_TYPE], "video/iso.segment");
        assert_eq!(segment, SEGMENT);
    }

    #[tokio::test]
    async fn force_hls_remuxes_a_file_the_profile_plays_directly_with_a_forced_hls_reason() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, playback) = start_with(&server, &token, media_file_id, true).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["method"], "remux");
        assert_eq!(
            playback["reasons"],
            json!([{ "kind": "forcedHls", "value": null }])
        );
        assert!(
            app.hls
                .folder(playback_id(&playback))
                .join("main.m3u8")
                .exists()
        );
    }

    #[tokio::test]
    async fn a_file_whose_video_needs_a_transcode_returns_the_needs_transcode_problem() {
        let mut app = TestApp::new().await;
        app.state.read_keyframes = ReadKeyframes::new(|_| {
            Box::pin(async { Ok(vec![Duration::ZERO, Duration::from_secs(30)]) })
        });
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (sparse_keyframes, _sparse_file) =
            add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, sparse_keyframes, true).await;
        let (hevc, _hevc_file) = add_media_file(&app, "mp4", Some(MP4)).await;
        add_movie_playing(&app, hevc, true).await;
        let mut db_context = app.state.db_context.clone();
        VideoStream::filter_by_media_file_id(hevc)
            .update()
            .codec("hevc")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let server = serve(&app).await;

        for media_file_id in [sparse_keyframes, hevc] {
            let (status, problem) = start(&server, &token, media_file_id).await;

            assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
            assert_eq!(problem["type"], "urn:selfless:problem:needs-transcode");
        }
    }

    fn audio_targets(app: &TestApp) -> Vec<(i64, Option<AudioTranscode>, bool)> {
        app.hls
            .sessions()
            .last()
            .unwrap()
            .audio
            .iter()
            .map(|x| (x.stream.index, x.transcode, x.selected))
            .collect()
    }

    fn eac3(channels: i64) -> Option<AudioTranscode> {
        Some(AudioTranscode {
            codec: AudioTranscodeCodec::Eac3,
            channels,
        })
    }

    fn aac(channels: i64) -> Option<AudioTranscode> {
        Some(AudioTranscode {
            codec: AudioTranscodeCodec::Aac,
            channels,
        })
    }

    #[tokio::test]
    async fn audio_a_profile_with_e_ac_3_does_not_cover_becomes_e_ac_3_and_the_rest_is_copied() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file_with_audio(
            &app,
            "mkv",
            Some("matroska,webm"),
            &[
                (1, "dts", 6, "eng", "DTS-HD MA 5.1", true),
                (2, "dts", 2, "eng", "Director's commentary", false),
                (3, "aac", 2, "fre", "Stereo", false),
            ],
        )
        .await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, playback) = start_on(&server, &token, media_file_id, apple_tv_profile()).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["method"], "transcode");
        assert_eq!(
            playback["reasons"],
            json!([
                { "kind": "container", "value": "mkv" },
                { "kind": "audioCodec", "value": "dts" },
            ])
        );
        assert!(
            playback["url"].as_str().unwrap().ends_with("/v1/main.m3u8"),
            "{}",
            playback["url"]
        );
        assert_eq!(
            audio_targets(&app),
            [(1, eac3(6), true), (2, eac3(2), false), (3, None, false)]
        );
    }

    #[tokio::test]
    async fn audio_a_profile_without_e_ac_3_does_not_cover_becomes_aac_up_to_its_maximum_channels()
    {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file_with_audio(
            &app,
            "mkv",
            Some("matroska,webm"),
            &[
                (1, "eac3", 6, "eng", "Surround 5.1", true),
                (2, "ac3", 6, "eng", "Surround 5.1", false),
                (3, "truehd", 8, "eng", "Atmos", false),
            ],
        )
        .await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, playback) = start_on(
            &server,
            &token,
            media_file_id,
            google_tv_profile_without_e_ac_3(),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["method"], "transcode");
        assert_eq!(
            playback["reasons"],
            json!([
                { "kind": "audioCodec", "value": "eac3" },
                { "kind": "audioCodec", "value": "truehd" },
            ])
        );
        assert_eq!(
            audio_targets(&app),
            [(1, aac(6), true), (2, None, false), (3, aac(6), false)]
        );
    }

    #[tokio::test]
    async fn audio_a_browser_does_not_cover_becomes_aac_with_at_most_two_channels() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file_with_audio(
            &app,
            "mp4",
            Some(MP4),
            &[
                (1, "eac3", 6, "eng", "Surround 5.1", true),
                (2, "aac", 6, "eng", "Surround 5.1", false),
                (3, "aac", 1, "eng", "Mono", false),
            ],
        )
        .await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        let (status, playback) = start(&server, &token, media_file_id).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(playback["method"], "transcode");
        assert_eq!(
            playback["reasons"],
            json!([
                { "kind": "audioCodec", "value": "eac3" },
                { "kind": "audioChannels", "value": "6" },
            ])
        );
        assert_eq!(
            audio_targets(&app),
            [(1, aac(2), true), (2, aac(2), false), (3, None, false)]
        );
    }

    #[tokio::test]
    async fn the_keyframes_are_read_on_the_first_remux_and_again_once_the_file_changes() {
        let mut app = TestApp::new().await;
        let reads = Arc::new(AtomicUsize::new(0));
        let counted = reads.clone();
        app.state.read_keyframes = ReadKeyframes::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(vec![Duration::ZERO, Duration::from_secs(6)]) })
        });
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;

        start(&server, &token, media_file_id).await;
        start(&server, &token, media_file_id).await;
        let before_change = reads.load(Ordering::SeqCst);
        let mut db_context = app.state.db_context.clone();
        MediaFile::filter_by_id(media_file_id)
            .update()
            .size(Some(11))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let (status, playback) = start(&server, &token, media_file_id).await;

        assert_eq!(before_change, 1);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        assert_eq!(
            (status, &playback["method"]),
            (StatusCode::CREATED, &json!("remux"))
        );
    }

    #[tokio::test]
    async fn ending_an_hls_playback_deletes_its_segments_and_its_urls_return_404() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, NAME, PASSWORD).await;
        let (media_file_id, _file) = add_media_file(&app, "mkv", Some("matroska,webm")).await;
        add_movie_playing(&app, media_file_id, true).await;
        let server = serve(&app).await;
        let (_, stopped) = start(&server, &token, media_file_id).await;
        report(&server, &token, &stopped, true).await;
        let (_, replaced) = start(&server, &token, media_file_id).await;
        let (_, current) = start(&server, &token, media_file_id).await;

        for playback in [&stopped, &replaced] {
            for file_name in ["main.m3u8", "video-0.m4s"] {
                let (status, _, problem) = server.get_bytes(&sibling(playback, file_name)).await;

                assert_eq!(status, StatusCode::NOT_FOUND, "{file_name}");
                let problem = serde_json::from_slice::<Value>(&problem).unwrap();
                assert_eq!(problem["type"], "urn:selfless:problem:not-found");
            }
            assert!(!app.hls.folder(playback_id(playback)).exists());
        }
        assert_eq!(
            server.get_bytes(&sibling(&current, "video-0.m4s")).await.0,
            StatusCode::OK
        );
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
                    Some(OtelValue::from("remux")),
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
            format!("/api/playbacks/{first_id}/media/not-the-signature/v1/main.m3u8"),
        ] {
            let (status, _, problem) = server.get_bytes(&url).await;

            assert_eq!(status, StatusCode::FORBIDDEN, "{url}");
            let problem = serde_json::from_slice::<Value>(&problem).unwrap();
            assert_eq!(problem["type"], "urn:selfless:problem:invalid-signature");
        }
    }
}
