use application::ports::AudioStream;
use application::ports::HdrType;
use application::ports::Probe;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SubtitleStream;
use application::ports::VideoStream;
use serde::Deserialize;
use std::time::Duration;
use tokio::process::Command;
use tracing::Instrument;
use tracing::field;

const IMAGE_SUBTITLE_CODECS: [&str; 4] =
    ["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "xsub"];

pub fn probe_media_file() -> ProbeMediaFile {
    ProbeMediaFile::new(|path| {
        let span = tracing::info_span!(
            "ffprobe",
            otel.name = "ffprobe",
            otel.status_code = field::Empty,
            process.executable.name = "ffprobe",
            process.exit.code = field::Empty,
            error.type = field::Empty,
        );
        Box::pin(probe(path, span.clone()).instrument(span))
    })
}

async fn probe(path: String, span: tracing::Span) -> Result<Probe, ProbeError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(&path)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| {
            span.record("otel.status_code", "ERROR");
            span.record("error.type", "spawn");
            ProbeError::caused_by(error)
        })?;
    if let Some(code) = output.status.code() {
        span.record("process.exit.code", code);
    }
    if !output.status.success() {
        span.record("otel.status_code", "ERROR");
        span.record("error.type", "exit");
        return Err(ProbeError::caused_by(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    parse(&output.stdout)
}

#[derive(Deserialize)]
struct Output {
    format: Format,
    #[serde(default)]
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct Format {
    format_name: String,
    duration: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "codec_type", rename_all = "lowercase")]
enum Stream {
    Video {
        index: i64,
        codec_name: String,
        profile: Option<String>,
        width: i64,
        height: i64,
        avg_frame_rate: Option<String>,
        r_frame_rate: Option<String>,
        color_transfer: Option<String>,
        #[serde(default)]
        side_data_list: Vec<SideData>,
        #[serde(default)]
        disposition: Disposition,
    },
    Audio {
        index: i64,
        codec_name: String,
        channels: i64,
        #[serde(default)]
        tags: Tags,
        #[serde(default)]
        disposition: Disposition,
    },
    Subtitle {
        index: i64,
        codec_name: String,
        #[serde(default)]
        tags: Tags,
        #[serde(default)]
        disposition: Disposition,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct SideData {
    side_data_type: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Disposition {
    default: u8,
    forced: u8,
    hearing_impaired: u8,
    attached_pic: u8,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Tags {
    language: Option<String>,
    title: Option<String>,
}

impl Tags {
    fn language(&self) -> Option<String> {
        self.language
            .as_deref()
            .filter(|x| *x != "und")
            .map(str::to_lowercase)
    }
}

fn parse(output: &[u8]) -> Result<Probe, ProbeError> {
    let output = serde_json::from_slice::<Output>(output).map_err(ProbeError::caused_by)?;
    let mut probe = Probe {
        container: output.format.format_name,
        duration: output
            .format
            .duration
            .and_then(|x| x.parse::<f64>().ok())
            .map(|x| Duration::from_millis((x * 1000.0).round() as u64)),
        video_streams: Vec::new(),
        audio_streams: Vec::new(),
        subtitle_streams: Vec::new(),
    };
    for stream in output.streams {
        match stream {
            Stream::Video {
                disposition: Disposition {
                    attached_pic: 1, ..
                },
                ..
            }
            | Stream::Other => {}
            Stream::Video {
                index,
                codec_name,
                profile,
                width,
                height,
                avg_frame_rate,
                r_frame_rate,
                color_transfer,
                side_data_list,
                ..
            } => probe.video_streams.push(VideoStream {
                index,
                codec: codec_name,
                profile,
                width,
                height,
                frame_rate: [avg_frame_rate, r_frame_rate]
                    .iter()
                    .find_map(|x| frame_rate(x.as_deref()?)),
                hdr_type: if side_data_list
                    .iter()
                    .any(|x| x.side_data_type == "DOVI configuration record")
                {
                    HdrType::DolbyVision
                } else {
                    match color_transfer.as_deref() {
                        Some("smpte2084") => HdrType::Hdr10,
                        Some("arib-std-b67") => HdrType::Hlg,
                        _ => HdrType::Sdr,
                    }
                },
            }),
            Stream::Audio {
                index,
                codec_name,
                channels,
                tags,
                disposition,
            } => probe.audio_streams.push(AudioStream {
                index,
                codec: codec_name,
                language: tags.language(),
                channels,
                title: tags.title,
                default: disposition.default == 1,
            }),
            Stream::Subtitle {
                index,
                codec_name,
                tags,
                disposition,
            } => probe.subtitle_streams.push(SubtitleStream {
                index,
                text: !IMAGE_SUBTITLE_CODECS.contains(&codec_name.as_str()),
                codec: codec_name,
                language: tags.language(),
                title: tags.title,
                default: disposition.default == 1,
                forced: disposition.forced == 1,
                hearing_impaired: disposition.hearing_impaired == 1,
            }),
        }
    }
    Ok(probe)
}

fn frame_rate(rational: &str) -> Option<f64> {
    let (numerator, denominator) = rational.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (numerator > 0.0 && denominator > 0.0).then(|| numerator / denominator)
}

#[cfg(test)]
mod tests;
