use crate::language::TrackLanguage;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::Span;

const HEADER: &str = "WEBVTT\nX-TIMESTAMP-MAP=MPEGTS:0,LOCAL:00:00:00.000\n";

pub(crate) struct Rendition<'a> {
    pub(crate) language: Option<&'a str>,
    pub(crate) title: Option<&'a str>,
    pub(crate) forced: bool,
    pub(crate) hearing_impaired: bool,
}

pub(crate) const GROUP: &str = "subtitles";

pub(crate) fn playlist(rendition: usize) -> String {
    format!("subtitles-{rendition}.m3u8")
}

pub(crate) fn segment(rendition: usize, segment: usize) -> String {
    format!("subtitles-{rendition}-{segment}.vtt")
}

pub(crate) fn file(rendition: usize) -> String {
    format!("subtitles-{rendition}.vtt")
}

pub(crate) fn media_tags<'a>(renditions: impl Iterator<Item = Rendition<'a>>) -> String {
    let mut names = Vec::<String>::new();
    let mut tags = String::new();
    for (index, rendition) in renditions.enumerate() {
        let language = rendition.language.and_then(TrackLanguage::parse);
        let base = name(&rendition, language.as_ref());
        let mut name = base.clone();
        let mut count = 1;
        while names.contains(&name) {
            count += 1;
            name = format!("{base} {count}");
        }
        tags.push_str(&format!(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"{GROUP}\",NAME=\"{name}\""
        ));
        if let Some(language) = &language {
            tags.push_str(&format!(",LANGUAGE=\"{}\"", language.tag()));
        }
        tags.push_str(",DEFAULT=NO");
        if rendition.forced {
            tags.push_str(",FORCED=YES");
        }
        tags.push_str(&format!(",URI=\"{}\"\n", playlist(index)));
        names.push(name);
    }
    tags
}

fn name(rendition: &Rendition, language: Option<&TrackLanguage>) -> String {
    let language_name = match (language, rendition.language) {
        (Some(language), _) => language.name(),
        (None, Some(code)) => code,
        (None, None) => "Unknown",
    };
    let mut marks = Vec::new();
    marks.extend(language.and_then(TrackLanguage::region));
    marks.extend(
        rendition
            .title
            .filter(|x| !x.eq_ignore_ascii_case(language_name)),
    );
    if rendition.hearing_impaired {
        marks.push("SDH");
    }
    if rendition.forced {
        marks.push("Forced");
    }
    let name = if marks.is_empty() {
        language_name.to_owned()
    } else {
        format!("{language_name} ({})", marks.join(", "))
    };
    name.replace('"', "'")
}

pub(crate) struct Cue<'a> {
    pub(crate) start: Duration,
    pub(crate) end: Duration,
    block: &'a str,
}

pub(crate) fn cues(webvtt: &str) -> impl Iterator<Item = Cue<'_>> {
    webvtt.split("\n\n").filter_map(|block| {
        let block = block.trim_matches('\n');
        let (start, rest) = block.lines().find_map(|x| x.split_once(" --> "))?;
        let end = rest.split_whitespace().next()?;
        Some(Cue {
            start: timestamp(start)?,
            end: timestamp(end)?,
            block,
        })
    })
}

fn timestamp(text: &str) -> Option<Duration> {
    let (clock, milliseconds) = text.split_once('.')?;
    let mut seconds = 0;
    for part in clock.split(':') {
        seconds = seconds * 60 + part.parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(seconds) + Duration::from_millis(milliseconds.parse().ok()?))
}

pub(crate) fn webvtt<'a>(cues: impl Iterator<Item = Cue<'a>>) -> String {
    let mut file = HEADER.to_owned();
    for cue in cues {
        file.push('\n');
        file.push_str(cue.block);
        file.push('\n');
    }
    file
}

pub(crate) async fn convert(path: &Path) -> io::Result<String> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostdin",
            "-nostats",
            "-loglevel",
            "error",
            "-i",
        ])
        .arg(path)
        .args(["-c:s", "webvtt", "-f", "webvtt", "pipe:1"])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .inspect_err(|_| {
            Span::current().record("otel.status_code", "ERROR");
            Span::current().record("error.type", "spawn");
        })?;
    if let Some(code) = output.status.code() {
        Span::current().record("process.exit.code", code);
    }
    if !output.status.success() {
        Span::current().record("otel.status_code", "ERROR");
        Span::current().record("error.type", "exit");
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(webvtt(cues(&String::from_utf8_lossy(&output.stdout))))
}
