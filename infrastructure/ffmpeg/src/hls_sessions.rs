use crate::hls::Segments;
use crate::hls::VIDEO;
use crate::hls::audio_rendition;
use crate::hls::write_segments;
use application::attributes::SELFLESS_FFMPEG_PURPOSE;
use application::attributes::SELFLESS_FFMPEG_START_SEGMENT;
use application::attributes::selfless_ffmpeg_purpose;
use application::ports::EndHlsSession;
use application::ports::GetHlsFile;
use application::ports::HlsAudioTrack;
use application::ports::HlsError;
use application::ports::HlsSession;
use application::ports::StartHlsSession;
use application::ports::VideoStream;
use application::spans::SELFLESS_FFMPEG;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::fs;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::field;
use uuid::Uuid;

const MAIN: &str = "main.m3u8";
const JUST_AHEAD: usize = 4;

#[derive(Clone)]
pub struct HlsSessions {
    segments_dir: PathBuf,
    sessions: Arc<Mutex<HashMap<Uuid, Session>>>,
}

struct Session {
    folder: PathBuf,
    input: PathBuf,
    video: VideoStream,
    audio: Vec<HlsAudioTrack>,
    segments: Arc<Segments>,
    run: Option<Run>,
}

struct Run {
    start: usize,
    written: watch::Receiver<usize>,
    task: JoinHandle<()>,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Run {
    fn finished(&self) -> bool {
        self.written.has_changed().is_err()
    }

    fn covers(&self, segment: Option<usize>) -> bool {
        segment.is_none_or(|x| self.start <= x && x <= *self.written.borrow() + JUST_AHEAD)
    }
}

enum Requested {
    Playlist,
    Init,
    Segment(usize),
}

fn requested(name: &str) -> Option<(&str, Requested)> {
    if name == MAIN {
        return Some((VIDEO, Requested::Playlist));
    }
    let (stem, extension) = name.rsplit_once('.')?;
    match (extension, stem.split_once('-')) {
        ("m3u8", None) => Some((stem, Requested::Playlist)),
        ("mp4", None) => Some((stem, Requested::Init)),
        ("m4s", Some((rendition, index))) => {
            Some((rendition, Requested::Segment(index.parse().ok()?)))
        }
        _ => None,
    }
}

impl HlsSessions {
    pub async fn new(segments_dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&segments_dir).await?;
        let mut entries = fs::read_dir(&segments_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                fs::remove_dir_all(entry.path()).await?;
            } else {
                fs::remove_file(entry.path()).await?;
            }
        }
        Ok(Self {
            segments_dir,
            sessions: Arc::default(),
        })
    }

    pub fn start_hls_session(&self) -> StartHlsSession {
        let sessions = self.clone();
        StartHlsSession::new(move |session| {
            let sessions = sessions.clone();
            Box::pin(async move { sessions.start(session).await.map_err(HlsError::caused_by) })
        })
    }

    pub fn get_hls_file(&self) -> GetHlsFile {
        let sessions = self.clone();
        GetHlsFile::new(move |playback_id, name| {
            let sessions = sessions.clone();
            Box::pin(async move {
                sessions
                    .file(playback_id, &name)
                    .await
                    .map(|x| x.map(|x| x.to_string_lossy().into_owned()))
                    .map_err(HlsError::caused_by)
            })
        })
    }

    pub fn end_hls_session(&self) -> EndHlsSession {
        let sessions = self.clone();
        EndHlsSession::new(move |playback_id| {
            let sessions = sessions.clone();
            Box::pin(async move { sessions.end(playback_id).await })
        })
    }

    async fn start(&self, session: HlsSession) -> io::Result<()> {
        let folder = self.segments_dir.join(session.playback_id.to_string());
        fs::create_dir_all(&folder).await?;
        let segments = Segments::new(&session.keyframes, session.duration);
        let size = fs::metadata(&session.path).await?.len();
        let bandwidth = (size as f64 * 8.0 / session.duration.as_secs_f64().max(1.0)).ceil();
        let mut main = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n".to_owned();
        let mut names = HashSet::new();
        for (position, track) in session.audio.iter().enumerate() {
            let rendition = audio_rendition(position);
            let mut name = [&track.stream.language, &track.stream.title]
                .into_iter()
                .flatten()
                .map(|x| x.replace(['"', '\r', '\n'], ""))
                .collect::<Vec<_>>()
                .join(" - ");
            if name.is_empty() {
                name = format!("Audio {}", position + 1);
            }
            if !names.insert(name.clone()) {
                name = format!("{name} ({})", position + 1);
            }
            let language = track
                .stream
                .language
                .as_ref()
                .map(|x| format!("LANGUAGE=\"{x}\","))
                .unwrap_or_default();
            main.push_str(&format!(
                "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"{name}\",{language}DEFAULT={},AUTOSELECT=YES,CHANNELS=\"{}\",URI=\"{rendition}.m3u8\"\n",
                if track.selected { "YES" } else { "NO" },
                track
                    .transcode
                    .map_or(track.stream.channels, |x| x.channels),
            ));
            fs::write(
                folder.join(format!("{rendition}.m3u8")),
                segments.media_playlist(&rendition),
            )
            .await?;
        }
        main.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={}x{}{}\n{VIDEO}.m3u8\n",
            session.video.width,
            session.video.height,
            if session.audio.is_empty() {
                ""
            } else {
                ",AUDIO=\"audio\""
            },
        ));
        fs::write(
            folder.join(format!("{VIDEO}.m3u8")),
            segments.media_playlist(VIDEO),
        )
        .await?;
        fs::write(folder.join(MAIN), main).await?;
        let start = segments.segment_at(session.start_position);
        let mut started = Session {
            folder,
            input: PathBuf::from(session.path),
            video: session.video,
            audio: session.audio,
            segments: Arc::new(segments),
            run: None,
        };
        started.restart(start);
        self.sessions
            .lock()
            .expect("no thread panics while holding the HLS sessions")
            .insert(session.playback_id, started);
        Ok(())
    }

    async fn file(&self, playback_id: Uuid, name: &str) -> io::Result<Option<PathBuf>> {
        let Some((rendition, requested)) = requested(name) else {
            return Ok(None);
        };
        loop {
            let mut written = {
                let mut sessions = self
                    .sessions
                    .lock()
                    .expect("no thread panics while holding the HLS sessions");
                let Some(session) = sessions.get_mut(&playback_id) else {
                    return Ok(None);
                };
                let mut written = session.run.as_ref().map(|x| x.written.clone());
                if let Some(written) = &mut written {
                    written.borrow_and_update();
                }
                if rendition != VIDEO
                    && !(0..session.audio.len()).any(|x| audio_rendition(x) == rendition)
                {
                    return Ok(None);
                }
                let path = session.folder.join(name);
                if path.exists() {
                    return Ok(Some(path));
                }
                let segment = match requested {
                    Requested::Playlist => return Ok(None),
                    Requested::Init => None,
                    Requested::Segment(x) if x >= session.segments.len() => return Ok(None),
                    Requested::Segment(x) => Some(x),
                };
                match &session.run {
                    Some(run) if run.finished() && run.start == segment.unwrap_or(run.start) => {
                        return Err(io::Error::other(format!(
                            "ffmpeg exited without writing {name}"
                        )));
                    }
                    Some(run) if !run.finished() && run.covers(segment) => {}
                    _ => {
                        session.restart(segment.unwrap_or_default());
                        written = session.run.as_ref().map(|x| x.written.clone());
                    }
                }
                written.expect("a run is writing the segments")
            };
            let _ = written.changed().await;
        }
    }

    async fn end(&self, playback_id: Uuid) {
        let Some(mut session) = self
            .sessions
            .lock()
            .expect("no thread panics while holding the HLS sessions")
            .remove(&playback_id)
        else {
            return;
        };
        if let Some(mut run) = session.run.take() {
            run.task.abort();
            let _ = (&mut run.task).await;
        }
        if let Err(error) = fs::remove_dir_all(&session.folder).await
            && error.kind() != ErrorKind::NotFound
        {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                "failed to delete the segments of an HLS session"
            );
        }
    }
}

impl Session {
    fn restart(&mut self, start: usize) {
        self.run = None;
        let (sender, written) = watch::channel(start);
        let span = tracing::info_span!(
            "ffmpeg",
            otel.name = SELFLESS_FFMPEG,
            otel.kind = "internal",
            otel.status_code = field::Empty,
            { SELFLESS_FFMPEG_PURPOSE } = if self.audio.iter().any(|x| x.transcode.is_some()) {
                selfless_ffmpeg_purpose::TRANSCODE
            } else {
                selfless_ffmpeg_purpose::REMUX
            },
            { SELFLESS_FFMPEG_START_SEGMENT } = start as i64,
            process.executable.name = "ffmpeg",
            process.exit.code = field::Empty,
            error.type = field::Empty,
        );
        let input = self.input.clone();
        let video = self.video.clone();
        let audio = self.audio.clone();
        let segments = self.segments.clone();
        let folder = self.folder.clone();
        let task = tokio::spawn(
            async move {
                if let Err(error) =
                    write_segments(&input, &video, &audio, &segments, start, &folder, &sender).await
                {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "ffmpeg failed to write the HLS segments of a media file"
                    );
                }
            }
            .instrument(span),
        );
        self.run = Some(Run {
            start,
            written,
            task,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::read_keyframes;
    use application::ports::AudioStream;
    use domain::AudioTranscode;
    use domain::AudioTranscodeCodec;
    use domain::HdrType;
    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::TempDir;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const PLAYBACK_ID: Uuid = Uuid::from_u128(0x0190_f3c4_d3e4_7f5a_8b6c_7d8e_9f0a_1b2c);

    async fn session(start_position: Duration) -> HlsSession {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/media/keyframes.mkv");
        HlsSession {
            playback_id: PLAYBACK_ID,
            keyframes: read_keyframes(&path).await.unwrap(),
            path: path.to_string_lossy().into_owned(),
            duration: Duration::from_secs(40),
            video: VideoStream {
                index: 0,
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                width: 160,
                height: 90,
                frame_rate: Some(25.0),
                hdr_type: HdrType::Sdr,
            },
            audio: vec![HlsAudioTrack {
                stream: AudioStream {
                    index: 1,
                    codec: "eac3".to_owned(),
                    language: None,
                    channels: 1,
                    title: None,
                    default: true,
                },
                transcode: None,
                selected: true,
            }],
            start_position,
        }
    }

    async fn started(start_position: Duration) -> (TempDir, HlsSessions) {
        start(session(start_position).await).await
    }

    async fn start(session: HlsSession) -> (TempDir, HlsSessions) {
        let dir = TempDir::new().unwrap();
        let sessions = HlsSessions::new(dir.path().join("segments")).await.unwrap();
        sessions.start_hls_session().call(session).await.unwrap();
        (dir, sessions)
    }

    fn audio_track(
        language: Option<&str>,
        title: Option<&str>,
        transcode: Option<AudioTranscode>,
        selected: bool,
    ) -> HlsAudioTrack {
        HlsAudioTrack {
            stream: AudioStream {
                index: 1,
                codec: "eac3".to_owned(),
                language: language.map(str::to_owned),
                channels: 1,
                title: title.map(str::to_owned),
                default: false,
            },
            transcode,
            selected,
        }
    }

    async fn get(sessions: &HlsSessions, name: &str) -> Option<String> {
        sessions
            .get_hls_file()
            .call(PLAYBACK_ID, name)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_session_serves_its_playlists_and_every_segment_of_its_media_playlists() {
        let mut session = session(Duration::ZERO).await;
        session.audio = vec![
            audio_track(Some("eng"), Some("Surround"), None, false),
            audio_track(
                None,
                Some("Commentary \"live\""),
                Some(AudioTranscode {
                    codec: AudioTranscodeCodec::Aac,
                    channels: 1,
                }),
                true,
            ),
            audio_track(Some("eng"), Some("Surround"), None, false),
            audio_track(None, None, None, false),
        ];
        let (_dir, sessions) = start(session).await;

        let main = std::fs::read_to_string(get(&sessions, "main.m3u8").await.unwrap()).unwrap();
        let mut playlists = Vec::new();
        for name in ["video", "audio0", "audio1", "audio2", "audio3"] {
            let playlist = get(&sessions, &format!("{name}.m3u8")).await.unwrap();
            playlists.push(std::fs::read_to_string(playlist).unwrap());
        }
        let mut files = Vec::new();
        for rendition in ["video", "audio0", "audio1", "audio2", "audio3"] {
            files.push(format!("{rendition}.mp4"));
            files.extend((0..6).map(|x| format!("{rendition}-{x}.m4s")));
        }
        let mut served = Vec::new();
        for name in &files {
            served.push(
                get(&sessions, name)
                    .await
                    .is_some_and(|x| Path::new(&x).exists()),
            );
        }

        assert_eq!(
            main,
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"eng - Surround\",LANGUAGE=\"eng\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio0.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"Commentary live\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio1.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"eng - Surround (3)\",LANGUAGE=\"eng\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio2.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"Audio 4\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio3.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=45542,RESOLUTION=160x90,AUDIO=\"audio\"\n\
             video.m3u8\n"
        );
        assert_eq!(
            playlists
                .iter()
                .map(|x| x.matches("#EXTINF:").count())
                .collect::<Vec<_>>(),
            [6; 5]
        );
        assert!(playlists[2].contains("#EXT-X-MAP:URI=\"audio1.mp4\"\n"));
        assert!(playlists[2].contains("\naudio1-5.m4s\n"));
        assert_eq!(served, [true; 35]);
        for name in ["video-6.m4s", "audio4.m3u8", "audio4-0.m4s", "../segments"] {
            assert_eq!(get(&sessions, name).await, None, "{name}");
        }
    }

    #[tokio::test]
    async fn a_request_before_the_running_ffmpeg_restarts_it_at_that_segment_in_its_own_span() {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );
        let (dir, sessions) = started(Duration::from_secs(22)).await;
        let folder = dir.path().join("segments").join(PLAYBACK_ID.to_string());

        let last = get(&sessions, "video-5.m4s").await;
        let before_restart = folder.join("video-0.m4s").exists();
        let first = get(&sessions, "video-0.m4s")
            .await
            .is_some_and(|x| Path::new(&x).exists());
        sessions.end_hls_session().call(PLAYBACK_ID).await;

        assert!(last.is_some());
        assert!(!before_restart);
        assert!(first);
        let runs = exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| x.name == "selfless.ffmpeg")
            .map(|x| {
                [
                    KeyValue::new("selfless.ffmpeg.purpose", "remux"),
                    KeyValue::new("selfless.ffmpeg.start_segment", 0),
                    KeyValue::new("selfless.ffmpeg.start_segment", 3),
                ]
                .map(|attribute| x.attributes.contains(&attribute))
            })
            .collect::<Vec<_>>();
        assert_eq!(runs, [[true, false, true], [true, true, false]]);
    }

    #[tokio::test]
    async fn ending_a_session_stops_ffmpeg_and_deletes_its_folder() {
        let (dir, sessions) = started(Duration::ZERO).await;
        let folder = dir.path().join("segments").join(PLAYBACK_ID.to_string());
        let existed = folder.exists();

        sessions.end_hls_session().call(PLAYBACK_ID).await;

        assert!(existed);
        assert!(!folder.exists());
        assert_eq!(get(&sessions, "main.m3u8").await, None);
    }

    #[tokio::test]
    async fn the_segments_dir_is_emptied_when_the_sessions_start() {
        let dir = TempDir::new().unwrap();
        let segments_dir = dir.path().join("segments");
        std::fs::create_dir_all(segments_dir.join("an old playback")).unwrap();
        std::fs::write(segments_dir.join("an old playback/video-0.m4s"), b"old").unwrap();
        std::fs::write(segments_dir.join("stray file"), b"old").unwrap();

        HlsSessions::new(segments_dir.clone()).await.unwrap();

        assert_eq!(std::fs::read_dir(&segments_dir).unwrap().count(), 0);
    }
}
