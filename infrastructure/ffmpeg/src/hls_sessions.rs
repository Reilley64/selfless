use crate::hls::INIT;
use crate::hls::Segments;
use crate::hls::audio_init;
use crate::hls::audio_playlist;
use crate::hls::audio_segment;
use crate::hls::write_atomically;
use crate::hls::write_segments;
use crate::rendition::Rendition;
use crate::rendition::named;
use crate::subtitles;
use application::attributes::SELFLESS_FFMPEG_PURPOSE;
use application::attributes::SELFLESS_FFMPEG_START_SEGMENT;
use application::attributes::selfless_ffmpeg_purpose;
use application::ports::EndHlsSession;
use application::ports::GetHlsFile;
use application::ports::HlsAudioTrack;
use application::ports::HlsError;
use application::ports::HlsSession;
use application::ports::StartHlsSession;
use application::ports::SubtitleStream;
use application::ports::VideoStream;
use application::spans::SELFLESS_FFMPEG;
use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::fs;
use tokio::sync::OnceCell;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::Span;
use tracing::field;
use uuid::Uuid;

const MAIN: &str = "main.m3u8";
const VIDEO: &str = "video.m3u8";
const AUDIO_GROUP: &str = "audio";
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
    subtitle_streams: Vec<SubtitleStream>,
    subtitle_files: Vec<SubtitleFile>,
    segments: Arc<Segments>,
    run: Option<Run>,
}

struct SubtitleFile {
    path: PathBuf,
    converted: Arc<OnceCell<()>>,
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
    AudioInit(usize),
    AudioSegment(usize, usize),
    SubtitleSegment(usize, usize),
    SubtitleFile(usize),
}

impl Requested {
    fn parse(name: &str) -> Option<Self> {
        if name == MAIN || name == VIDEO {
            return Some(Self::Playlist);
        }
        if name == INIT {
            return Some(Self::Init);
        }
        if let Some(audio) = name.strip_prefix("audio-") {
            if let Some(rendition) = audio.strip_suffix(".m3u8") {
                return rendition.parse::<usize>().ok().map(|_| Self::Playlist);
            }
            if let Some(rendition) = audio.strip_suffix(".mp4") {
                return rendition.parse().ok().map(Self::AudioInit);
            }
            let (rendition, segment) = audio.strip_suffix(".m4s")?.split_once('-')?;
            return Some(Self::AudioSegment(
                rendition.parse().ok()?,
                segment.parse().ok()?,
            ));
        }
        if let Some(segment) = name.strip_suffix(".m4s") {
            return segment.parse().ok().map(Self::Segment);
        }
        let subtitle = name.strip_prefix("subtitles-")?;
        if let Some(rendition) = subtitle.strip_suffix(".m3u8") {
            return rendition.parse::<usize>().ok().map(|_| Self::Playlist);
        }
        let subtitle = subtitle.strip_suffix(".vtt")?;
        Some(match subtitle.split_once('-') {
            Some((rendition, segment)) => {
                Self::SubtitleSegment(rendition.parse().ok()?, segment.parse().ok()?)
            }
            None => Self::SubtitleFile(subtitle.parse().ok()?),
        })
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
        let renditions = session
            .subtitle_streams
            .iter()
            .map(|x| Rendition {
                language: x.language.as_deref(),
                title: x.title.as_deref(),
                forced: x.forced,
                hearing_impaired: x.hearing_impaired,
            })
            .chain(session.subtitle_files.iter().map(|x| Rendition {
                language: x.language.as_deref(),
                title: None,
                forced: x.forced,
                hearing_impaired: x.hearing_impaired,
            }));
        let mut audio_tags = String::new();
        let audio_renditions = named(session.audio.iter().map(|x| Rendition {
            language: x.stream.language.as_deref(),
            title: x.stream.title.as_deref(),
            forced: false,
            hearing_impaired: false,
        }));
        for (index, (track, rendition)) in session.audio.iter().zip(audio_renditions).enumerate() {
            audio_tags.push_str(&format!(
                "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"{AUDIO_GROUP}\",NAME=\"{}\"",
                rendition.name
            ));
            if let Some(language) = &rendition.language {
                audio_tags.push_str(&format!(",LANGUAGE=\"{language}\""));
            }
            audio_tags.push_str(&format!(
                ",DEFAULT={},AUTOSELECT=YES,CHANNELS=\"{}\",URI=\"{}\"\n",
                if track.selected { "YES" } else { "NO" },
                track
                    .transcode
                    .map_or(track.stream.channels, |x| x.channels),
                audio_playlist(index)
            ));
        }
        let mut groups = String::new();
        if !session.audio.is_empty() {
            groups.push_str(&format!(",AUDIO=\"{AUDIO_GROUP}\""));
        }
        if !session.subtitle_streams.is_empty() || !session.subtitle_files.is_empty() {
            groups.push_str(&format!(",SUBTITLES=\"{}\"", subtitles::GROUP));
        }
        fs::write(
            folder.join(MAIN),
            format!(
                "#EXTM3U\n\
                 #EXT-X-VERSION:7\n\
                 #EXT-X-INDEPENDENT-SEGMENTS\n\
                 {audio_tags}\
                 {}\
                 #EXT-X-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={}x{}{groups}\n\
                 {VIDEO}\n",
                subtitles::media_tags(renditions),
                session.video.width,
                session.video.height
            ),
        )
        .await?;
        fs::write(
            folder.join(VIDEO),
            segments.media_playlist(Some(INIT), |x| format!("{x}.m4s")),
        )
        .await?;
        for rendition in 0..session.audio.len() {
            fs::write(
                folder.join(audio_playlist(rendition)),
                segments.media_playlist(Some(&audio_init(rendition)), |x| {
                    audio_segment(rendition, x)
                }),
            )
            .await?;
        }
        let embedded = session.subtitle_streams.len();
        for rendition in 0..embedded {
            fs::write(
                folder.join(subtitles::playlist(rendition)),
                segments.media_playlist(None, |x| subtitles::segment(rendition, x)),
            )
            .await?;
        }
        let whole = Segments::new(&[], session.duration);
        for rendition in embedded..embedded + session.subtitle_files.len() {
            fs::write(
                folder.join(subtitles::playlist(rendition)),
                whole.media_playlist(None, |_| subtitles::file(rendition)),
            )
            .await?;
        }
        let start = segments.segment_at(session.start_position);
        let mut started = Session {
            folder,
            input: PathBuf::from(session.path),
            video: session.video,
            audio: session.audio,
            subtitle_streams: session.subtitle_streams,
            subtitle_files: session
                .subtitle_files
                .into_iter()
                .map(|x| SubtitleFile {
                    path: PathBuf::from(x.path),
                    converted: Arc::default(),
                })
                .collect(),
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
        let Some(requested) = Requested::parse(name) else {
            return Ok(None);
        };
        let (source, converted, path) = loop {
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
                let path = session.folder.join(name);
                if path.exists() {
                    return Ok(Some(path));
                }
                let embedded = session.subtitle_streams.len();
                let segment = match requested {
                    Requested::Playlist => return Ok(None),
                    Requested::AudioInit(rendition) | Requested::AudioSegment(rendition, _)
                        if rendition >= session.audio.len() =>
                    {
                        return Ok(None);
                    }
                    Requested::Init | Requested::AudioInit(_) => None,
                    Requested::Segment(x)
                    | Requested::AudioSegment(_, x)
                    | Requested::SubtitleSegment(_, x)
                        if x >= session.segments.len() =>
                    {
                        return Ok(None);
                    }
                    Requested::SubtitleSegment(rendition, _) if rendition >= embedded => {
                        return Ok(None);
                    }
                    Requested::Segment(x)
                    | Requested::AudioSegment(_, x)
                    | Requested::SubtitleSegment(_, x) => Some(x),
                    Requested::SubtitleFile(rendition) => {
                        let Some(file) = rendition
                            .checked_sub(embedded)
                            .and_then(|x| session.subtitle_files.get(x))
                        else {
                            return Ok(None);
                        };
                        break (file.path.clone(), file.converted.clone(), path);
                    }
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
        };
        converted
            .get_or_try_init(|| {
                async {
                    let webvtt = subtitles::convert(&source).await?;
                    write_atomically(&path, webvtt.as_bytes()).await
                }
                .instrument(ffmpeg_span(selfless_ffmpeg_purpose::SUBTITLE, 0))
            })
            .await?;
        Ok(Some(path))
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
        let purpose = if self.audio.iter().any(|x| x.transcode.is_some()) {
            selfless_ffmpeg_purpose::TRANSCODE
        } else {
            selfless_ffmpeg_purpose::REMUX
        };
        let (sender, written) = watch::channel(start);
        let input = self.input.clone();
        let video = self.video.clone();
        let audio = self.audio.clone();
        let subtitle_streams = self.subtitle_streams.clone();
        let segments = self.segments.clone();
        let folder = self.folder.clone();
        let task = tokio::spawn(
            async move {
                if let Err(error) = write_segments(
                    &input,
                    &video,
                    &audio,
                    &subtitle_streams,
                    &segments,
                    start,
                    &folder,
                    &sender,
                )
                .await
                {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "ffmpeg failed to write the HLS segments of a media file"
                    );
                }
            }
            .instrument(ffmpeg_span(purpose, start)),
        );
        self.run = Some(Run {
            start,
            written,
            task,
        });
    }
}

fn ffmpeg_span(purpose: &'static str, start: usize) -> Span {
    tracing::info_span!(
        "ffmpeg",
        otel.name = SELFLESS_FFMPEG,
        otel.kind = "internal",
        otel.status_code = field::Empty,
        { SELFLESS_FFMPEG_PURPOSE } = purpose,
        { SELFLESS_FFMPEG_START_SEGMENT } = start as i64,
        process.executable.name = "ffmpeg",
        process.exit.code = field::Empty,
        error.type = field::Empty,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::read_keyframes;
    use application::ports::AudioStream;
    use application::ports::FoundSubtitleFile;
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
            audio: vec![audio_track(None, None, None, true)],
            subtitle_streams: Vec::new(),
            subtitle_files: Vec::new(),
            start_position,
        }
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

    fn media(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/fixtures/media")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn subtitle_stream(index: i64, codec: &str, language: &str) -> SubtitleStream {
        SubtitleStream {
            index,
            codec: codec.to_owned(),
            language: Some(language.to_owned()),
            title: None,
            default: false,
            forced: false,
            hearing_impaired: false,
            text: true,
        }
    }

    async fn subtitle_session(start_position: Duration) -> HlsSession {
        let path = media("subtitles.mkv");
        HlsSession {
            keyframes: read_keyframes(Path::new(&path)).await.unwrap(),
            path,
            subtitle_streams: vec![
                SubtitleStream {
                    title: Some("English".to_owned()),
                    ..subtitle_stream(2, "subrip", "eng")
                },
                SubtitleStream {
                    forced: true,
                    ..subtitle_stream(3, "ass", "fre")
                },
            ],
            subtitle_files: vec![
                FoundSubtitleFile {
                    path: media("subtitles.en.srt"),
                    language: Some("en".to_owned()),
                    hearing_impaired: false,
                    forced: false,
                },
                FoundSubtitleFile {
                    path: media("subtitles.fr.ass"),
                    language: Some("fr".to_owned()),
                    hearing_impaired: true,
                    forced: false,
                },
            ],
            ..session(start_position).await
        }
    }

    async fn started(start_position: Duration) -> (TempDir, HlsSessions) {
        started_with(session(start_position).await).await
    }

    async fn started_with(session: HlsSession) -> (TempDir, HlsSessions) {
        let dir = TempDir::new().unwrap();
        let sessions = HlsSessions::new(dir.path().join("segments")).await.unwrap();
        sessions.start_hls_session().call(session).await.unwrap();
        (dir, sessions)
    }

    async fn read(sessions: &HlsSessions, name: &str) -> String {
        std::fs::read_to_string(get(sessions, name).await.unwrap()).unwrap()
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
        let (_dir, sessions) = started(Duration::ZERO).await;

        let main = read(&sessions, "main.m3u8").await;
        let video = read(&sessions, "video.m3u8").await;
        let audio = read(&sessions, "audio-0.m3u8").await;
        let mut files = vec!["init.mp4".to_owned(), "audio-0.mp4".to_owned()];
        files.extend((0..6).map(|x| format!("{x}.m4s")));
        files.extend((0..6).map(|x| format!("audio-0-{x}.m4s")));
        let mut served = Vec::new();
        for name in &files {
            served.push(
                get(&sessions, name)
                    .await
                    .is_some_and(|x| Path::new(&x).exists()),
            );
        }

        assert!(main.contains("#EXT-X-STREAM-INF:BANDWIDTH="), "{main}");
        assert!(
            main.ends_with(",RESOLUTION=160x90,AUDIO=\"audio\"\nvideo.m3u8\n"),
            "{main}"
        );
        assert_eq!(video.matches("#EXTINF:").count(), 6);
        assert!(
            audio.contains("#EXT-X-MAP:URI=\"audio-0.mp4\"\n"),
            "{audio}"
        );
        assert!(audio.contains("\naudio-0-5.m4s\n"), "{audio}");
        assert_eq!(served, [true; 14]);
        for name in [
            "6.m4s",
            "audio-0-6.m4s",
            "audio-1.mp4",
            "audio-1-0.m4s",
            "../segments",
        ] {
            assert_eq!(get(&sessions, name).await, None, "{name}");
        }
    }

    #[tokio::test]
    async fn the_main_playlist_has_an_audio_group_with_every_audio_track_and_the_selected_one_as_default()
     {
        let mut session = session(Duration::ZERO).await;
        session.audio = vec![
            audio_track(Some("eng"), Some("Surround 5.1"), None, false),
            audio_track(
                Some("fre"),
                Some("Commentary \"live\""),
                Some(AudioTranscode {
                    codec: AudioTranscodeCodec::Aac,
                    channels: 1,
                }),
                true,
            ),
            audio_track(Some("eng"), Some("Surround 5.1"), None, false),
            audio_track(None, None, None, false),
        ];
        let (_dir, sessions) = started_with(session).await;

        let main = read(&sessions, "main.m3u8").await;
        let playlists = read_all(&sessions, (0..4).map(|x| format!("audio-{x}.m3u8"))).await;

        assert_eq!(
            main,
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"English (Surround 5.1)\",LANGUAGE=\"en\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio-0.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"French (Commentary 'live')\",LANGUAGE=\"fr\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio-1.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"English (Surround 5.1) 2\",LANGUAGE=\"en\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio-2.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"Unknown\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio-3.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=45542,RESOLUTION=160x90,AUDIO=\"audio\"\n\
             video.m3u8\n"
        );
        assert_eq!(
            playlists
                .iter()
                .map(|x| x.matches("#EXTINF:").count())
                .collect::<Vec<_>>(),
            [6; 4]
        );
    }

    #[tokio::test]
    async fn the_main_playlist_has_a_subtitle_group_with_every_text_subtitle_named_by_language_and_title()
     {
        let (_dir, sessions) = started_with(subtitle_session(Duration::ZERO).await).await;

        let main = read(&sessions, "main.m3u8").await;

        assert_eq!(
            main,
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"Unknown\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"1\",URI=\"audio-0.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subtitles\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=NO,URI=\"subtitles-0.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subtitles\",NAME=\"French (Forced)\",LANGUAGE=\"fr\",DEFAULT=NO,FORCED=YES,URI=\"subtitles-1.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subtitles\",NAME=\"English 2\",LANGUAGE=\"en\",DEFAULT=NO,URI=\"subtitles-2.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subtitles\",NAME=\"French (SDH)\",LANGUAGE=\"fr\",DEFAULT=NO,URI=\"subtitles-3.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=45775,RESOLUTION=160x90,AUDIO=\"audio\",SUBTITLES=\"subtitles\"\n\
             video.m3u8\n"
        );
    }

    fn webvtt(cues: &[&str]) -> String {
        let mut file = "WEBVTT\nX-TIMESTAMP-MAP=MPEGTS:0,LOCAL:00:00:00.000\n".to_owned();
        for cue in cues {
            file.push_str(&format!("\n{cue}\n"));
        }
        file
    }

    async fn read_all(sessions: &HlsSessions, names: impl Iterator<Item = String>) -> Vec<String> {
        let mut files = Vec::new();
        for name in names {
            files.push(read(sessions, &name).await);
        }
        files
    }

    #[tokio::test]
    async fn embedded_text_subtitles_become_webvtt_segments_on_the_playlist_times_also_after_a_restart()
     {
        let (_dir, from_start) = started_with(subtitle_session(Duration::ZERO).await).await;
        let (_restarted_dir, restarted) =
            started_with(subtitle_session(Duration::from_secs(22)).await).await;
        let lines = |playlist: &str, prefix: &str| {
            playlist
                .lines()
                .filter(|x| x.starts_with(prefix))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };

        let video = read(&from_start, "video.m3u8").await;
        let playlist = read(&from_start, "subtitles-0.m3u8").await;
        let segments = read_all(&from_start, (0..6).map(|x| format!("subtitles-0-{x}.vtt"))).await;
        let after_restart =
            read_all(&restarted, (3..6).map(|x| format!("subtitles-0-{x}.vtt"))).await;
        let ass = read(&from_start, "subtitles-1-0.vtt").await;

        assert_eq!(lines(&playlist, "#EXTINF:"), lines(&video, "#EXTINF:"));
        assert_eq!(
            lines(&playlist, "subtitles-"),
            (0..6)
                .map(|x| format!("subtitles-0-{x}.vtt"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            segments,
            [
                webvtt(&[
                    "00:01.000 --> 00:03.000\nOne",
                    "00:05.000 --> 00:08.000\nAcross the first cut",
                ]),
                webvtt(&["00:05.000 --> 00:08.000\nAcross the first cut"]),
                webvtt(&["00:14.000 --> 00:16.000\nTwo"]),
                webvtt(&[]),
                webvtt(&["00:31.000 --> 00:33.500\nThree"]),
                webvtt(&["00:37.000 --> 00:39.000\nLast"]),
            ]
        );
        assert_eq!(after_restart, segments[3..]);
        assert_eq!(ass, webvtt(&["00:02.000 --> 00:04.000\nUn"]));
    }

    #[tokio::test]
    async fn srt_and_ass_files_beside_the_video_become_one_webvtt_file_in_a_one_segment_playlist() {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );
        let (_dir, sessions) = started_with(subtitle_session(Duration::ZERO).await).await;

        let playlists = read_all(&sessions, (2..4).map(|x| format!("subtitles-{x}.m3u8"))).await;
        let files = read_all(&sessions, (2..4).map(|x| format!("subtitles-{x}.vtt"))).await;

        assert_eq!(
            playlists,
            (2..4)
                .map(|x| format!(
                    "#EXTM3U\n\
                     #EXT-X-VERSION:7\n\
                     #EXT-X-TARGETDURATION:40\n\
                     #EXT-X-MEDIA-SEQUENCE:0\n\
                     #EXT-X-PLAYLIST-TYPE:VOD\n\
                     #EXT-X-INDEPENDENT-SEGMENTS\n\
                     #EXTINF:40.000000,\n\
                     subtitles-{x}.vtt\n\
                     #EXT-X-ENDLIST\n"
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            files,
            [
                webvtt(&[
                    "00:02.500 --> 00:04.000\nBeside the video",
                    "00:35.000 --> 00:38.000\nNear the end",
                ]),
                webvtt(&["00:10.000 --> 00:12.000\nEn haut"]),
            ]
        );
        assert_eq!(get(&sessions, "subtitles-4.vtt").await, None);
        let conversions = exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| {
                x.name == "selfless.ffmpeg"
                    && x.attributes
                        .contains(&KeyValue::new("selfless.ffmpeg.purpose", "subtitle"))
                    && x.attributes
                        .contains(&KeyValue::new("selfless.ffmpeg.start_segment", 0))
            })
            .count();
        assert_eq!(conversions, 2);
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

        let last = get(&sessions, "5.m4s").await;
        let before_restart = folder.join("0.m4s").exists();
        let first = get(&sessions, "0.m4s")
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
        std::fs::write(segments_dir.join("an old playback/0.m4s"), b"old").unwrap();
        std::fs::write(segments_dir.join("stray file"), b"old").unwrap();

        HlsSessions::new(segments_dir.clone()).await.unwrap();

        assert_eq!(std::fs::read_dir(&segments_dir).unwrap().count(), 0);
    }
}
