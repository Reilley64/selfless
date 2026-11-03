use crate::subtitles;
use application::ports::HlsAudioTrack;
use application::ports::SubtitleStream;
use application::ports::VideoStream;
use domain::AudioTranscodeCodec;
use domain::VideoTranscode;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::Span;

const SEGMENT_MARK: Duration = Duration::from_secs(6);
const DEFAULT_FRAME_RATE: f64 = 25.0;
const HARDWARE_DECODED_CODECS: [&str; 5] = ["hevc", "mpeg2video", "vc1", "vp9", "av1"];
const HARDWARE_DECODED_H264_PROFILES: [&str; 4] =
    ["Constrained Baseline", "Baseline", "Main", "High"];
pub(crate) const INIT: &str = "init.mp4";
const SEGMENT_FORMAT_OPTIONS: &str = "movflags=+frag_custom+empty_moov+delay_moov+default_base_moof+frag_discont+negative_cts_offsets+skip_trailer:strict=unofficial";

pub struct Segments {
    starts: Vec<Duration>,
    duration: Duration,
}

impl Segments {
    pub fn new(keyframes: &[Duration], duration: Duration) -> Self {
        let mut starts = vec![keyframes.first().copied().unwrap_or_default()];
        let mut mark = SEGMENT_MARK;
        for &keyframe in keyframes.iter().take_while(|x| **x < duration) {
            if keyframe >= mark {
                starts.push(keyframe);
                while mark <= keyframe {
                    mark += SEGMENT_MARK;
                }
            }
        }
        Self { starts, duration }
    }

    pub fn fixed(frame_rate: Option<f64>, duration: Duration) -> Self {
        let frame_rate = frame_rate.unwrap_or(DEFAULT_FRAME_RATE);
        let length = Duration::from_secs_f64(gop(frame_rate) as f64 / frame_rate);
        let starts = (0..)
            .map(|x| length * x)
            .take_while(|x| x.is_zero() || *x < duration)
            .collect();
        Self { starts, duration }
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn segment_at(&self, position: Duration) -> usize {
        self.starts
            .iter()
            .skip(1)
            .take_while(|x| **x <= position)
            .count()
    }

    pub fn bounds(&self) -> impl Iterator<Item = (Duration, Duration)> {
        [Duration::ZERO]
            .into_iter()
            .chain(self.starts.iter().skip(1).copied())
            .zip(self.starts.iter().skip(1).copied().chain([self.duration]))
    }

    pub fn media_playlist(&self, map: Option<&str>, segment: impl Fn(usize) -> String) -> String {
        let lengths = self
            .bounds()
            .map(|(start, end)| end.saturating_sub(start))
            .collect::<Vec<_>>();
        let target = lengths
            .iter()
            .map(|x| x.as_secs_f64().ceil() as u64)
            .max()
            .unwrap_or_default();
        let mut playlist = format!(
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-TARGETDURATION:{target}\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n"
        );
        if let Some(map) = map {
            playlist.push_str(&format!("#EXT-X-MAP:URI=\"{map}\"\n"));
        }
        for (index, length) in lengths.iter().enumerate() {
            playlist.push_str(&format!(
                "#EXTINF:{:.6},\n{}\n",
                length.as_secs_f64(),
                segment(index)
            ));
        }
        playlist.push_str("#EXT-X-ENDLIST\n");
        playlist
    }
}

#[derive(Clone)]
pub struct SegmentWriter {
    pub input: PathBuf,
    pub video: VideoStream,
    pub video_transcode: Option<VideoTranscode>,
    pub audio: Vec<HlsAudioTrack>,
    pub subtitles: Vec<SubtitleStream>,
    pub segments: Arc<Segments>,
    pub folder: PathBuf,
}

impl SegmentWriter {
    pub async fn write(&self, segment: usize, written: &watch::Sender<usize>) -> io::Result<()> {
        let Self {
            input,
            video,
            video_transcode,
            audio,
            subtitles,
            segments,
            folder,
        } = self;
        let first = segments.starts[segment];
        let start = if segment == 0 { Duration::ZERO } else { first };
        let cuts = segments.starts[segment + 1..]
            .iter()
            .map(|x| seconds(*x - first))
            .collect::<Vec<_>>()
            .join(",");
        let segment_output = |command: &mut Command, pattern: &str| {
            command
                .args(["-copypriorss", "0", "-avoid_negative_ts", "disabled"])
                .args(["-f", "segment", "-segment_format", "mp4"])
                .args(["-segment_format_options", SEGMENT_FORMAT_OPTIONS])
                .args(["-segment_times", &cuts, "-segment_time_delta", "0.001"])
                .args(["-segment_start_number", &segment.to_string()])
                .args(["-segment_list", "pipe:1", "-segment_list_type", "flat"])
                .arg(folder.join(pattern));
        };
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error"]);
        let hardware_decoded = match video.codec.as_str() {
            "h264" => video
                .profile
                .as_deref()
                .is_some_and(|x| HARDWARE_DECODED_H264_PROFILES.contains(&x)),
            codec => HARDWARE_DECODED_CODECS.contains(&codec),
        };
        if video_transcode.is_some() {
            command
                .args(["-init_hw_device", "vaapi=va:/dev/dri/renderD128,driver=iHD"])
                .args(["-init_hw_device", "qsv=qs@va", "-filter_hw_device", "qs"]);
            if hardware_decoded {
                command.args(["-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi"]);
            }
        }
        command
            .args(["-ss", &seconds(start), "-copyts", "-i"])
            .arg(input)
            .args(["-map", &format!("0:{}", video.index)]);
        match video_transcode {
            Some(transcode) => {
                let mut filter = if hardware_decoded {
                    "hwmap=derive_device=qsv".to_owned()
                } else {
                    "format=nv12,hwupload=extra_hw_frames=64".to_owned()
                };
                filter.push_str(&format!(
                    ",vpp_qsv=w={}:h={}:format=nv12",
                    transcode.width, transcode.height
                ));
                if transcode.deinterlace {
                    filter.push_str(":deinterlace=2");
                }
                if transcode.tone_map {
                    filter.push_str(
                    ":tonemap=1:out_color_matrix=bt709:out_color_primaries=bt709:out_color_transfer=bt709",
                );
                }
                let gop = gop(video.frame_rate.unwrap_or(DEFAULT_FRAME_RATE)).to_string();
                command
                    .args(["-vf", &filter, "-c:v", "h264_qsv", "-low_power", "1"])
                    .args(["-preset", "veryfast", "-profile:v", "high"])
                    .args(["-b:v", &transcode.bit_rate.to_string()])
                    .args(["-maxrate", &transcode.bit_rate.to_string()])
                    .args(["-bufsize", &(transcode.bit_rate * 2).to_string()])
                    .args(["-g", &gop, "-keyint_min", &gop, "-bf", "0"]);
            }
            None => {
                command.args(["-c", "copy"]);
                if video.codec == "hevc" {
                    command.args(["-tag:v", "hvc1"]);
                }
            }
        }
        segment_output(&mut command, "%d.mp4");
        let mut progress = HashMap::from([(String::new(), segment)]);
        for (rendition, track) in audio.iter().enumerate() {
            command.args(["-map", &format!("0:{}", track.stream.index)]);
            match track.transcode {
                None => command.args(["-c", "copy"]),
                Some(transcode) => command.args([
                    "-c",
                    match transcode.codec {
                        AudioTranscodeCodec::Eac3 => "eac3",
                        AudioTranscodeCodec::Aac => "aac",
                    },
                    "-ac",
                    &transcode.channels.to_string(),
                ]),
            };
            command.args(["-bsf:a", "noise=drop=lt(pts\\,0)"]);
            segment_output(&mut command, &format!("{AUDIO}-{rendition}-%d.mp4"));
            progress.insert(format!("{AUDIO}-{rendition}"), segment);
        }
        let subtitle_runs = (0..subtitles.len())
            .map(|x| folder.join(format!("subtitles-{x}-from-{segment}.webvtt")))
            .collect::<Vec<_>>();
        for (subtitle, run) in subtitles.iter().zip(&subtitle_runs) {
            command
                .args(["-map", &format!("0:{}", subtitle.index), "-c:s", "webvtt"])
                .args(["-flush_packets", "1", "-f", "webvtt"])
                .arg(run);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .inspect_err(|_| {
                Span::current().record("otel.status_code", "ERROR");
                Span::current().record("error.type", "spawn");
            })?;
        let mut finished = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
        let mut last = HashMap::new();
        while let Some(next) = finished.next_line().await? {
            let (rendition, _) = segment_name(&next)?;
            if let Some(name) = last.insert(rendition.to_owned(), next) {
                finish_segment(folder, &name, &subtitle_runs, segments, &mut progress).await?;
                written.send_replace(progress.values().copied().min().unwrap_or(segment));
            }
        }
        let output = child.wait_with_output().await?;
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
        for name in last.into_values() {
            finish_segment(folder, &name, &subtitle_runs, segments, &mut progress).await?;
        }
        written.send_replace(progress.values().copied().min().unwrap_or(segment));
        Ok(())
    }
}

const AUDIO: &str = "audio";

pub(crate) fn audio_playlist(rendition: usize) -> String {
    format!("{AUDIO}-{rendition}.m3u8")
}

pub(crate) fn audio_init(rendition: usize) -> String {
    format!("{AUDIO}-{rendition}.mp4")
}

pub(crate) fn audio_segment(rendition: usize, segment: usize) -> String {
    format!("{AUDIO}-{rendition}-{segment}.m4s")
}

fn segment_name(name: &str) -> io::Result<(&str, usize)> {
    name.strip_suffix(".mp4")
        .map(|x| x.rsplit_once('-').unwrap_or(("", x)))
        .and_then(|(rendition, index)| Some((rendition, index.parse().ok()?)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected segment name"))
}

async fn finish_segment(
    folder: &Path,
    name: &str,
    subtitle_runs: &[PathBuf],
    segments: &Segments,
    progress: &mut HashMap<String, usize>,
) -> io::Result<()> {
    let (rendition, index) = segment_name(name)?;
    let path = folder.join(name);
    let bytes = fs::read(&path).await?;
    let header = header_length(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "segment without moov"))?;
    let init = if rendition.is_empty() {
        folder.join(INIT)
    } else {
        folder.join(format!("{rendition}.mp4"))
    };
    if !fs::try_exists(&init).await? {
        write_atomically(&init, &bytes[..header]).await?;
    }
    write_atomically(&path.with_extension("m4s"), &bytes[header..]).await?;
    fs::remove_file(path).await?;
    if rendition.is_empty() {
        write_subtitle_segments(folder, subtitle_runs, segments, index).await?;
    }
    progress.insert(rendition.to_owned(), index + 1);
    Ok(())
}

async fn write_subtitle_segments(
    folder: &Path,
    runs: &[PathBuf],
    segments: &Segments,
    index: usize,
) -> io::Result<()> {
    let Some((start, end)) = segments.bounds().nth(index) else {
        return Ok(());
    };
    for (rendition, run) in runs.iter().enumerate() {
        let run = fs::read_to_string(run).await?;
        let cues = subtitles::cues(&run).filter(|x| x.start < end && x.end > start);
        write_atomically(
            &folder.join(subtitles::segment(rendition, index)),
            subtitles::webvtt(cues).as_bytes(),
        )
        .await?;
    }
    Ok(())
}

fn header_length(segment: &[u8]) -> Option<usize> {
    let mut position = 0;
    while let Some(header) = segment.get(position..position + 8) {
        let size = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
        position = position.checked_add(size).filter(|_| size >= 8)?;
        if &header[4..] == b"moov" {
            return Some(position);
        }
    }
    None
}

pub(crate) async fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let partial = path.with_extension("partial");
    fs::write(&partial, bytes).await?;
    fs::rename(partial, path).await
}

fn gop(frame_rate: f64) -> u64 {
    (SEGMENT_MARK.as_secs_f64() * frame_rate).round() as u64
}

fn seconds(duration: Duration) -> String {
    format!("{}.{:06}", duration.as_secs(), duration.subsec_micros())
}

#[cfg(test)]
mod tests {
    use super::SegmentWriter;
    use super::Segments;
    use super::audio_init;
    use super::audio_segment;
    use crate::keyframes::read_keyframes;
    use application::ports::AudioStream;
    use application::ports::HlsAudioTrack;
    use application::ports::VideoStream;
    use domain::AudioTranscode;
    use domain::AudioTranscodeCodec;
    use domain::HdrType;
    use mp4_atom::Any;
    use mp4_atom::Decode;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    const FIXTURE_DURATION: Duration = Duration::from_secs(40);
    const TWO_E_AC_3_FRAMES_MS: u64 = 64;

    fn seconds(values: &[f64]) -> Vec<Duration> {
        values.iter().map(|x| Duration::from_secs_f64(*x)).collect()
    }

    #[test]
    fn the_media_playlist_cuts_at_the_first_keyframe_at_or_after_each_6_second_mark() {
        let segments = Segments::new(
            &seconds(&[0.0, 4.4, 6.0, 7.2, 13.0, 14.4, 21.6, 22.0, 30.0, 36.0]),
            Duration::from_secs(40),
        );

        assert_eq!(
            segments.media_playlist(Some("init.mp4"), |x| format!("{x}.m4s")),
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-TARGETDURATION:9\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:6.000000,\n0.m4s\n\
             #EXTINF:7.000000,\n1.m4s\n\
             #EXTINF:8.600000,\n2.m4s\n\
             #EXTINF:8.400000,\n3.m4s\n\
             #EXTINF:6.000000,\n4.m4s\n\
             #EXTINF:4.000000,\n5.m4s\n\
             #EXT-X-ENDLIST\n"
        );
    }

    #[test]
    fn a_transcode_cuts_after_each_6_second_gop_of_whole_frames() {
        let playlist = |frame_rate| {
            playlist_times(
                &Segments::fixed(frame_rate, Duration::from_secs(20))
                    .media_playlist(Some("init.mp4"), |x| format!("{x}.m4s")),
            )
        };

        assert_eq!(
            playlist(Some(24000.0 / 1001.0)),
            [(0, 6006), (6006, 12012), (12012, 18018), (18018, 20000)]
        );
        assert_eq!(
            playlist(Some(25.0)),
            [(0, 6000), (6000, 12000), (12000, 18000), (18000, 20000)]
        );
    }

    #[test]
    fn a_position_falls_in_the_last_segment_that_starts_at_or_before_it() {
        let segments = Segments::new(&seconds(&[0.0, 6.0, 13.0, 21.6]), Duration::from_secs(30));

        let at = |x| segments.segment_at(Duration::from_secs_f64(x));

        assert_eq!(
            [at(0.0), at(5.9), at(6.0), at(21.5), at(29.0)],
            [0, 0, 1, 2, 3]
        );
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/fixtures/media")
            .join(name)
    }

    fn playlist_times(playlist: &str) -> Vec<(i64, i64)> {
        let mut start = 0;
        playlist
            .lines()
            .filter_map(|x| x.strip_prefix("#EXTINF:")?.strip_suffix(','))
            .map(|x| {
                let end = start + (x.parse::<f64>().unwrap() * 1000.0).round() as i64;
                let times = (start, end);
                start = end;
                times
            })
            .collect()
    }

    fn atoms(path: &Path) -> Vec<Any> {
        let bytes = std::fs::read(path).unwrap();
        let mut buffer = bytes.as_slice();
        let mut atoms = Vec::new();
        while !buffer.is_empty() {
            atoms.push(Any::decode(&mut buffer).unwrap());
        }
        atoms
    }

    fn segment_times(
        folder: &Path,
        init: &str,
        segment: impl Fn(usize) -> String,
        segments: impl Iterator<Item = usize>,
    ) -> Vec<(i64, i64)> {
        let (track_id, timescale) = atoms(&folder.join(init))
            .into_iter()
            .find_map(|x| match x {
                Any::Moov(moov) => moov
                    .trak
                    .first()
                    .map(|x| (x.tkhd.track_id, i64::from(x.mdia.mdhd.timescale))),
                _ => None,
            })
            .expect("the init segment has a track");
        segments
            .map(|index| {
                let mut start = i64::MAX;
                let mut end = i64::MIN;
                for atom in atoms(&folder.join(segment(index))) {
                    let Any::Moof(moof) = atom else { continue };
                    for traf in moof.traf.iter().filter(|x| x.tfhd.track_id == track_id) {
                        let mut decode_time =
                            traf.tfdt.as_ref().unwrap().base_media_decode_time as i64;
                        for sample in traf.trun.iter().flat_map(|x| x.samples.iter()) {
                            let duration = i64::from(
                                sample
                                    .duration
                                    .or(traf.tfhd.default_sample_duration)
                                    .unwrap(),
                            );
                            let presentation = decode_time + sample.cts.unwrap_or(0);
                            start = start.min(presentation);
                            end = end.max(presentation + duration);
                            decode_time += duration;
                        }
                    }
                }
                let milliseconds = |x: i64| (x * 1000 + timescale / 2).div_euclid(timescale);
                (milliseconds(start), milliseconds(end))
            })
            .collect()
    }

    async fn write_fixture_segments(
        name: &str,
        audio_codec: &str,
        transcode: Option<AudioTranscode>,
        segment: usize,
    ) -> (Arc<Segments>, TempDir) {
        let input = fixture(name);
        let keyframes = read_keyframes(&input).await.unwrap();
        let segments = Segments::new(&keyframes, FIXTURE_DURATION);
        let folder = TempDir::new().unwrap();
        let video = VideoStream {
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("High".to_owned()),
            level: None,
            bit_depth: None,
            width: 160,
            height: 90,
            frame_rate: Some(25.0),
            hdr_type: HdrType::Sdr,
            dolby_vision_profile: None,
            dolby_vision_compatibility_id: None,
            interlaced: false,
            bit_rate: None,
        };
        let audio = HlsAudioTrack {
            stream: AudioStream {
                index: 1,
                codec: audio_codec.to_owned(),
                language: None,
                channels: 1,
                title: None,
                default: true,
            },
            transcode,
            selected: true,
        };
        let writer = SegmentWriter {
            input,
            video,
            video_transcode: None,
            audio: vec![audio],
            subtitles: Vec::new(),
            segments: Arc::new(segments),
            folder: folder.path().to_owned(),
        };
        writer
            .write(segment, &tokio::sync::watch::channel(segment).0)
            .await
            .unwrap();
        (writer.segments, folder)
    }

    async fn assert_remux_matches_the_playlist(name: &str, audio_codec: &str) {
        let (segments, from_start) = write_fixture_segments(name, audio_codec, None, 0).await;
        let (_, restarted) = write_fixture_segments(name, audio_codec, None, 3).await;
        let playlist =
            playlist_times(&segments.media_playlist(Some("init.mp4"), |x| format!("{x}.m4s")));

        assert_eq!(playlist.len(), 6);
        assert_eq!(
            segment_times(from_start.path(), "init.mp4", |x| format!("{x}.m4s"), 0..6),
            playlist
        );
        assert_eq!(
            segment_times(restarted.path(), "init.mp4", |x| format!("{x}.m4s"), 3..6),
            playlist[3..]
        );
        for init in ["init.mp4", "audio-0.mp4"] {
            assert_eq!(
                std::fs::read(restarted.path().join(init)).unwrap(),
                std::fs::read(from_start.path().join(init)).unwrap(),
                "{init}"
            );
        }
    }

    #[tokio::test]
    async fn a_remux_of_a_matroska_file_with_e_ac_3_audio_matches_the_playlist_from_the_start_and_after_a_restart()
     {
        assert_remux_matches_the_playlist("keyframes.mkv", "eac3").await;
    }

    #[tokio::test]
    async fn a_remux_of_an_mp4_file_matches_the_playlist_from_the_start_and_after_a_restart() {
        assert_remux_matches_the_playlist("keyframes.mp4", "aac").await;
    }

    async fn decode(path: &Path) -> (String, String) {
        let probe = tokio::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,channels"])
            .args(["-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(path)
            .output()
            .await
            .unwrap();
        let decoded = tokio::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .output()
            .await
            .unwrap();
        assert!(decoded.status.success());
        (
            String::from_utf8(probe.stdout)
                .unwrap()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(","),
            String::from_utf8(decoded.stderr).unwrap(),
        )
    }

    async fn assert_audio_transcode_plays_and_matches_the_playlist(
        name: &str,
        audio_codec: &str,
        transcode: AudioTranscode,
        decoded_as: &str,
    ) {
        let (segments, from_start) =
            write_fixture_segments(name, audio_codec, Some(transcode), 0).await;
        let (_, restarted) = write_fixture_segments(name, audio_codec, Some(transcode), 3).await;
        let playlist =
            playlist_times(&segments.media_playlist(Some(&audio_init(0)), |x| audio_segment(0, x)));
        let video = segment_times(from_start.path(), "init.mp4", |x| format!("{x}.m4s"), 0..6);
        let mut joined = std::fs::read(from_start.path().join(audio_init(0))).unwrap();
        for (folder, index) in [(&from_start, 0..3), (&restarted, 3..6)]
            .into_iter()
            .flat_map(|(folder, range)| range.map(move |x| (folder, x)))
        {
            joined.extend(std::fs::read(folder.path().join(audio_segment(0, index))).unwrap());
        }
        let joined_path = from_start.path().join("joined.mp4");
        std::fs::write(&joined_path, joined).unwrap();

        let times = [
            segment_times(
                from_start.path(),
                &audio_init(0),
                |x| audio_segment(0, x),
                0..6,
            ),
            segment_times(
                restarted.path(),
                &audio_init(0),
                |x| audio_segment(0, x),
                3..6,
            ),
        ];
        let (codec, errors) = decode(&joined_path).await;

        let off_by = |times: &[(i64, i64)], expected: &[(i64, i64)]| {
            times
                .iter()
                .zip(expected)
                .flat_map(|(x, y)| [x.0.abs_diff(y.0), x.1.abs_diff(y.1)])
                .max()
                .unwrap()
        };
        assert!(times[0][0].0 >= 0, "{times:?}");
        assert!(
            off_by(&times[0], &playlist) <= TWO_E_AC_3_FRAMES_MS,
            "{times:?} {playlist:?}"
        );
        assert!(
            off_by(&times[1], &playlist[3..]) <= TWO_E_AC_3_FRAMES_MS,
            "{times:?} {playlist:?}"
        );
        assert_eq!(codec, decoded_as);
        assert_eq!(errors, "");
        assert_eq!(video, playlist);
    }

    #[tokio::test]
    async fn an_audio_only_transcode_of_e_ac_3_to_aac_plays_and_matches_the_playlist_after_a_restart()
     {
        assert_audio_transcode_plays_and_matches_the_playlist(
            "keyframes.mkv",
            "eac3",
            AudioTranscode {
                codec: AudioTranscodeCodec::Aac,
                channels: 1,
            },
            "aac,1",
        )
        .await;
    }

    #[tokio::test]
    async fn an_audio_only_transcode_of_aac_to_e_ac_3_plays_and_matches_the_playlist_after_a_restart()
     {
        assert_audio_transcode_plays_and_matches_the_playlist(
            "keyframes.mp4",
            "aac",
            AudioTranscode {
                codec: AudioTranscodeCodec::Eac3,
                channels: 1,
            },
            "eac3,1",
        )
        .await;
    }
}
