use domain::PlayMethod;
use domain::PlayReason;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Playback(Box<Playback>),
    NotFound,
    UnreadableMediaFile,
    NeedsHls,
}

#[derive(Debug)]
pub struct Playback {
    pub id: Uuid,
    pub method: PlayMethod,
    pub reasons: Vec<PlayReason>,
    pub signature: String,
    pub duration: Option<Duration>,
    pub movie: Option<Movie>,
    pub episodes: Vec<Episode>,
    pub audio_tracks: Vec<AudioTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}

#[derive(Debug)]
pub struct Movie {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug)]
pub struct Episode {
    pub id: Uuid,
    pub show_title: String,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: String,
}

#[derive(Debug)]
pub struct AudioTrack {
    pub id: Uuid,
    pub language: Option<String>,
    pub label: Option<String>,
    pub codec: String,
    pub channels: i64,
    pub default: bool,
    pub in_stream: bool,
}

#[derive(Debug)]
pub struct SubtitleTrack {
    pub id: Uuid,
    pub language: Option<String>,
    pub label: Option<String>,
    pub codec: String,
    pub default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub external: bool,
    pub text: bool,
    pub in_stream: bool,
}
