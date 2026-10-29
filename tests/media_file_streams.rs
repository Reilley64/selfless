mod support;

use application::get_media_file::MediaFile;
use application::ports::AudioStream;
use application::ports::HdrType;
use application::ports::Probe;
use application::ports::SubtitleFile;
use application::ports::SubtitleStream;
use application::ports::VideoStream;
use application::sync_status::SyncStatus;
use domain::Source;
use jiff::SignedDuration;
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use support::FakeClock;
use support::FakeProber;
use support::MediaFolder;
use support::StubRadarr;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::settings;

const ALL_MOVIES: [i64; 4] = [42, 43, 44, 45];
const ARRIVAL: i64 = 42;

struct Setup {
    database: TestDatabase,
    radarr: StubRadarr,
    sonarr: StubSonarr,
    media: MediaFolder,
    prober: FakeProber,
}

struct Fixture {
    setup: Setup,
    server: TestServer,
}

impl Setup {
    async fn new() -> Self {
        let setup = Self {
            database: TestDatabase::create().await,
            radarr: StubRadarr::start().await,
            sonarr: StubSonarr::start().await,
            media: MediaFolder::new(),
            prober: FakeProber::new(),
        };
        setup.sonarr.serve_series_list("[]").await;
        setup
    }

    async fn radarr_lists(&self, ids: &[i64]) -> Vec<String> {
        let movies: Vec<Value> =
            serde_json::from_str(&self.media.fixture("radarr/api/movies.json")).unwrap();
        let movies = movies
            .into_iter()
            .filter(|x| ids.contains(&x["id"].as_i64().unwrap()))
            .collect::<Vec<_>>();
        let paths = movies
            .iter()
            .map(|x| x["movieFile"]["path"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        for path in &paths {
            self.media.create_absolute(path);
        }
        self.radarr
            .serve_movies(StatusCode::OK, &Value::Array(movies).to_string())
            .await;
        paths
    }

    async fn sonarr_lists_severance_and_andor(&self) -> Vec<String> {
        let series = ["", "-andor"]
            .map(|x| self.media.fixture(&format!("sonarr/api/series{x}.json")))
            .join(",");
        self.sonarr.serve_series_list(&format!("[{series}]")).await;
        let mut paths = Vec::new();
        for (id, suffix) in [(7, ""), (8, "-andor")] {
            let files = self
                .media
                .fixture(&format!("sonarr/api/episode-files{suffix}.json"));
            for file in serde_json::from_str::<Vec<Value>>(&files).unwrap() {
                let path = file["path"].as_str().unwrap().to_owned();
                self.media.create_absolute(&path);
                paths.push(path);
            }
            let episodes = self
                .media
                .fixture(&format!("sonarr/api/episodes{suffix}.json"));
            self.sonarr
                .serve_episodes(id, &episodes, &files, Duration::ZERO)
                .await;
        }
        paths
    }

    async fn start(self) -> Fixture {
        let mut settings = settings(self.database.url());
        settings.radarr.url = Some(self.radarr.url());
        settings.sonarr.url = Some(self.sonarr.url());
        let server =
            TestServer::start_with_prober(settings, FakeClock::new(), self.prober.clone()).await;
        let fixture = Fixture {
            setup: self,
            server,
        };
        fixture.reconciled(Source::Radarr).await;
        fixture.reconciled(Source::Sonarr).await;
        fixture
    }
}

impl Fixture {
    async fn status(&self) -> SyncStatus {
        application::sync_status::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn reconciled(&self, source: Source) {
        let started_at = self.server.clock.now();
        eventually(async || {
            let status = self.status().await;
            let status = match source {
                Source::Radarr => status.radarr,
                Source::Sonarr => status.sonarr,
            };
            status
                .last_reconcile
                .is_some_and(|x| x.started_at == started_at)
                .then_some(())
        })
        .await;
    }

    async fn sync_now(&self) {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::sync_now::call(&self.server.state);
        self.reconciled(Source::Radarr).await;
        self.reconciled(Source::Sonarr).await;
    }

    async fn movie_file(&self) -> MediaFile {
        let movie = application::list_movies::call(&self.server.state)
            .await
            .unwrap()
            .pop()
            .unwrap();
        application::get_media_file::call(&self.server.state, movie.media_file_id.unwrap())
            .await
            .unwrap()
            .unwrap()
    }

    async fn stop(self) {
        drop(self.server);
        self.setup.database.drop().await;
    }
}

fn hdr10_movie() -> Probe {
    Probe {
        container: "matroska,webm".to_owned(),
        duration: Some(Duration::from_millis(6_981_000)),
        video_streams: vec![VideoStream {
            index: 0,
            codec: "hevc".to_owned(),
            profile: Some("Main 10".to_owned()),
            width: 3840,
            height: 1600,
            frame_rate: Some(24000.0 / 1001.0),
            hdr_type: HdrType::Hdr10,
        }],
        audio_streams: vec![AudioStream {
            index: 1,
            codec: "truehd".to_owned(),
            language: Some("eng".to_owned()),
            channels: 8,
            title: Some("Dolby TrueHD Atmos 7.1".to_owned()),
            default: true,
        }],
        subtitle_streams: vec![
            SubtitleStream {
                index: 2,
                codec: "subrip".to_owned(),
                language: Some("eng".to_owned()),
                title: Some("SDH".to_owned()),
                default: false,
                forced: false,
                hearing_impaired: true,
                text: true,
            },
            SubtitleStream {
                index: 3,
                codec: "hdmv_pgs_subtitle".to_owned(),
                language: Some("eng".to_owned()),
                title: Some("Heptapod".to_owned()),
                default: true,
                forced: true,
                hearing_impaired: false,
                text: false,
            },
        ],
    }
}

fn sdr_movie() -> Probe {
    Probe {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration: Some(Duration::from_millis(6_980_500)),
        video_streams: vec![VideoStream {
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("High".to_owned()),
            width: 1920,
            height: 800,
            frame_rate: Some(24000.0 / 1001.0),
            hdr_type: HdrType::Sdr,
        }],
        audio_streams: vec![AudioStream {
            index: 1,
            codec: "aac".to_owned(),
            language: None,
            channels: 2,
            title: None,
            default: true,
        }],
        subtitle_streams: Vec::new(),
    }
}

fn beside(video: &str, suffix: &str) -> String {
    format!("{}{suffix}", video.strip_suffix(".mkv").unwrap())
}

fn subtitle_file(
    path: String,
    language: Option<&str>,
    hearing_impaired: bool,
    forced: bool,
) -> SubtitleFile {
    SubtitleFile {
        path,
        language: language.map(str::to_owned),
        hearing_impaired,
        forced,
    }
}

#[tokio::test]
async fn a_new_file_is_probed_once_and_its_streams_are_stored() {
    let setup = Setup::new().await;
    let video = setup.radarr_lists(&[ARRIVAL]).await.remove(0);
    setup.prober.serve(&video, hdr10_movie());
    let fixture = setup.start().await;

    fixture.sync_now().await;

    let media_file = fixture.movie_file().await;
    let probe = hdr10_movie();
    assert_eq!(media_file.container, Some(probe.container));
    assert_eq!(media_file.duration, probe.duration);
    assert_eq!(media_file.size, Some(0));
    assert_eq!(media_file.video_streams, probe.video_streams);
    assert_eq!(media_file.audio_streams, probe.audio_streams);
    assert_eq!(media_file.subtitle_streams, probe.subtitle_streams);
    assert_eq!(fixture.setup.prober.calls(&video), 1);
    fixture.stop().await;
}

#[tokio::test]
async fn a_file_whose_size_or_modification_time_changed_is_probed_again() {
    let setup = Setup::new().await;
    let video = setup.radarr_lists(&[ARRIVAL]).await.remove(0);
    setup.prober.serve(&video, hdr10_movie());
    let fixture = setup.start().await;

    fixture.setup.media.change_absolute(&video);
    fixture.setup.prober.serve(&video, sdr_movie());
    fixture.sync_now().await;

    let media_file = fixture.movie_file().await;
    assert_eq!(media_file.container, Some(sdr_movie().container));
    assert_eq!(media_file.video_streams, sdr_movie().video_streams);
    assert_eq!(media_file.audio_streams, sdr_movie().audio_streams);
    assert_eq!(media_file.subtitle_streams, []);
    assert_eq!(fixture.setup.prober.calls(&video), 2);
    fixture.stop().await;
}

#[tokio::test]
async fn at_most_four_files_are_probed_at_a_time() {
    let setup = Setup::new().await;
    let movies = setup.radarr_lists(&ALL_MOVIES).await;
    let episodes = setup.sonarr_lists_severance_and_andor().await;
    let files = movies.len() + episodes.len();
    setup.prober.hold();
    let prober = setup.prober.clone();
    let mut settings = settings(setup.database.url());
    settings.radarr.url = Some(setup.radarr.url());
    settings.sonarr.url = Some(setup.sonarr.url());
    let server = TestServer::start_with_prober(settings, FakeClock::new(), prober.clone()).await;

    eventually(async || (prober.running() == 4).then_some(())).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(prober.running(), 4);
    prober.release();
    let fixture = Fixture { setup, server };
    fixture.reconciled(Source::Radarr).await;
    fixture.reconciled(Source::Sonarr).await;

    assert!(files > 4);
    assert_eq!(prober.total_calls(), files);
    assert_eq!(prober.most_running(), 4);
    fixture.stop().await;
}

#[tokio::test]
async fn subtitle_files_count_when_their_name_is_the_video_name_then_a_language_and_flags() {
    let setup = Setup::new().await;
    let video = setup.radarr_lists(&[ARRIVAL]).await.remove(0);
    let orphan = video.replace("Bluray-1080p.mkv", "WEBDL-720p.en.srt");
    for path in [
        beside(&video, ".en.srt"),
        beside(&video, ".en.hi.srt"),
        beside(&video, ".hi.srt"),
        beside(&video, ".fr.forced.ass"),
        beside(&video, ".pt-BR.srt"),
        beside(&video, ".en.sdh.srt"),
        beside(&video, ".sdh.srt"),
        beside(&video, ".nfo"),
        orphan,
    ] {
        setup.media.create_absolute(&path);
    }
    let fixture = setup.start().await;

    assert_eq!(
        fixture.movie_file().await.subtitle_files,
        [
            subtitle_file(beside(&video, ".en.hi.srt"), Some("en"), true, false),
            subtitle_file(beside(&video, ".en.sdh.srt"), Some("en"), true, false),
            subtitle_file(beside(&video, ".en.srt"), Some("en"), false, false),
            subtitle_file(beside(&video, ".fr.forced.ass"), Some("fr"), false, true),
            subtitle_file(beside(&video, ".hi.srt"), None, true, false),
            subtitle_file(beside(&video, ".pt-BR.srt"), Some("pt-BR"), false, false),
            subtitle_file(beside(&video, ".sdh.srt"), None, true, false),
        ]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_picks_up_added_and_removed_subtitle_files() {
    let setup = Setup::new().await;
    let video = setup.radarr_lists(&[ARRIVAL]).await.remove(0);
    setup.media.create_absolute(&beside(&video, ".en.srt"));
    let fixture = setup.start().await;
    assert_eq!(fixture.movie_file().await.subtitle_files.len(), 1);

    fixture
        .setup
        .media
        .remove_absolute(&beside(&video, ".en.srt"));
    fixture
        .setup
        .media
        .create_absolute(&beside(&video, ".de.srt"));
    fixture.sync_now().await;

    assert_eq!(
        fixture.movie_file().await.subtitle_files,
        [subtitle_file(
            beside(&video, ".de.srt"),
            Some("de"),
            false,
            false
        )]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_file_ffprobe_cannot_read_stays_visible_without_stream_data_until_it_changes() {
    let setup = Setup::new().await;
    let video = setup.radarr_lists(&[ARRIVAL]).await.remove(0);
    setup.prober.fail(&video);
    let fixture = setup.start().await;

    let media_file = fixture.movie_file().await;
    assert_eq!(media_file.container, None);
    assert_eq!(media_file.video_streams, []);
    fixture.sync_now().await;
    assert_eq!(fixture.setup.prober.calls(&video), 1);

    fixture.setup.media.change_absolute(&video);
    fixture.setup.prober.serve(&video, hdr10_movie());
    fixture.sync_now().await;

    assert_eq!(
        fixture.movie_file().await.video_streams,
        hdr10_movie().video_streams
    );
    fixture.stop().await;
}
