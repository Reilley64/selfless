use application::ports::HlsAudioTrack;
use application::ports::VideoStream;
use domain::AudioTranscodeCodec;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::Span;

const SEGMENT_MARK: Duration = Duration::from_secs(6);
pub(crate) const VIDEO: &str = "video";
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

    pub fn media_playlist(&self, rendition: &str) -> String {
        let lengths = [&Duration::ZERO]
            .into_iter()
            .chain(self.starts.iter().skip(1))
            .zip(self.starts.iter().skip(1).chain([&self.duration]))
            .map(|(start, end)| end.saturating_sub(*start))
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
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MAP:URI=\"{rendition}.mp4\"\n"
        );
        for (index, length) in lengths.iter().enumerate() {
            playlist.push_str(&format!(
                "#EXTINF:{:.6},\n{rendition}-{index}.m4s\n",
                length.as_secs_f64()
            ));
        }
        playlist.push_str("#EXT-X-ENDLIST\n");
        playlist
    }
}

pub async fn write_segments(
    input: &Path,
    video: &VideoStream,
    audio: &[HlsAudioTrack],
    segments: &Segments,
    segment: usize,
    folder: &Path,
    written: &watch::Sender<usize>,
) -> io::Result<()> {
    let first = segments.starts[segment];
    let start = if segment == 0 { Duration::ZERO } else { first };
    let cuts = segments.starts[segment + 1..]
        .iter()
        .map(|x| seconds(*x - first))
        .collect::<Vec<_>>()
        .join(",");
    let segment_output = |command: &mut Command, rendition: &str| {
        command
            .args(["-copypriorss", "0", "-avoid_negative_ts", "disabled"])
            .args(["-f", "segment", "-segment_format", "mp4"])
            .args(["-segment_format_options", SEGMENT_FORMAT_OPTIONS])
            .args(["-segment_times", &cuts, "-segment_time_delta", "0.001"])
            .args(["-segment_start_number", &segment.to_string()])
            .args(["-segment_list", "pipe:1", "-segment_list_type", "flat"])
            .arg(folder.join(format!("{rendition}-%d.mp4")));
    };
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error"])
        .args(["-ss", &seconds(start), "-copyts", "-i"])
        .arg(input)
        .args(["-map", &format!("0:{}", video.index), "-c", "copy"]);
    if video.codec == "hevc" {
        command.args(["-tag:v", "hvc1"]);
    }
    segment_output(&mut command, VIDEO);
    let mut progress = HashMap::from([(VIDEO.to_owned(), segment)]);
    for (position, track) in audio.iter().enumerate() {
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
        let rendition = audio_rendition(position);
        segment_output(&mut command, &rendition);
        progress.insert(rendition, segment);
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
            split_segment(folder, &name, &mut progress).await?;
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
        split_segment(folder, &name, &mut progress).await?;
    }
    written.send_replace(progress.values().copied().min().unwrap_or(segment));
    Ok(())
}

pub fn audio_rendition(position: usize) -> String {
    format!("audio{position}")
}

fn segment_name(name: &str) -> io::Result<(&str, usize)> {
    name.strip_suffix(".mp4")
        .and_then(|x| x.rsplit_once('-'))
        .and_then(|(rendition, index)| Some((rendition, index.parse().ok()?)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected segment name"))
}

async fn split_segment(
    folder: &Path,
    name: &str,
    progress: &mut HashMap<String, usize>,
) -> io::Result<()> {
    let (rendition, index) = segment_name(name)?;
    let path = folder.join(name);
    let bytes = fs::read(&path).await?;
    let header = header_length(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "segment without moov"))?;
    let init = folder.join(format!("{rendition}.mp4"));
    if !fs::try_exists(&init).await? {
        write_atomically(&init, &bytes[..header]).await?;
    }
    write_atomically(&path.with_extension("m4s"), &bytes[header..]).await?;
    fs::remove_file(path).await?;
    progress.insert(rendition.to_owned(), index + 1);
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

async fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let partial = path.with_extension("partial");
    fs::write(&partial, bytes).await?;
    fs::rename(partial, path).await
}

fn seconds(duration: Duration) -> String {
    format!("{}.{:06}", duration.as_secs(), duration.subsec_micros())
}

#[cfg(test)]
mod tests {
    use super::Segments;
    use super::write_segments;
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
    use std::time::Duration;
    use tempfile::TempDir;

    const FIXTURE_DURATION: Duration = Duration::from_secs(40);
    const AUDIO_FRAME_MS: u64 = 32;

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
            segments.media_playlist("video"),
            "#EXTM3U\n\
             #EXT-X-VERSION:7\n\
             #EXT-X-TARGETDURATION:9\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MAP:URI=\"video.mp4\"\n\
             #EXTINF:6.000000,\nvideo-0.m4s\n\
             #EXTINF:7.000000,\nvideo-1.m4s\n\
             #EXTINF:8.600000,\nvideo-2.m4s\n\
             #EXTINF:8.400000,\nvideo-3.m4s\n\
             #EXTINF:6.000000,\nvideo-4.m4s\n\
             #EXTINF:4.000000,\nvideo-5.m4s\n\
             #EXT-X-ENDLIST\n"
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
        rendition: &str,
        segments: impl Iterator<Item = usize>,
    ) -> Vec<(i64, i64)> {
        let (track_id, timescale) = atoms(&folder.join(format!("{rendition}.mp4")))
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
                for atom in atoms(&folder.join(format!("{rendition}-{index}.m4s"))) {
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
    ) -> (Segments, TempDir) {
        let input = fixture(name);
        let keyframes = read_keyframes(&input).await.unwrap();
        let segments = Segments::new(&keyframes, FIXTURE_DURATION);
        let folder = TempDir::new().unwrap();
        let video = VideoStream {
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("High".to_owned()),
            width: 160,
            height: 90,
            frame_rate: Some(25.0),
            hdr_type: HdrType::Sdr,
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
        write_segments(
            &input,
            &video,
            &[audio],
            &segments,
            segment,
            folder.path(),
            &tokio::sync::watch::channel(segment).0,
        )
        .await
        .unwrap();
        (segments, folder)
    }

    async fn assert_remux_matches_the_playlist(name: &str, audio_codec: &str) {
        let (segments, from_start) = write_fixture_segments(name, audio_codec, None, 0).await;
        let (_, restarted) = write_fixture_segments(name, audio_codec, None, 3).await;
        let playlist = playlist_times(&segments.media_playlist("video"));

        assert_eq!(playlist.len(), 6);
        assert_eq!(segment_times(from_start.path(), "video", 0..6), playlist);
        assert_eq!(
            segment_times(restarted.path(), "video", 3..6),
            playlist[3..]
        );
        for init in ["video.mp4", "audio0.mp4"] {
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
        let playlist = playlist_times(&segments.media_playlist("audio0"));
        let video = segment_times(from_start.path(), "video", 0..6);
        let mut joined = std::fs::read(from_start.path().join("audio0.mp4")).unwrap();
        for (folder, index) in [(&from_start, 0..3), (&restarted, 3..6)]
            .into_iter()
            .flat_map(|(folder, range)| range.map(move |x| (folder, x)))
        {
            joined
                .extend(std::fs::read(folder.path().join(format!("audio0-{index}.m4s"))).unwrap());
        }
        let joined_path = from_start.path().join("joined.mp4");
        std::fs::write(&joined_path, joined).unwrap();

        let times = [
            segment_times(from_start.path(), "audio0", 0..6),
            segment_times(restarted.path(), "audio0", 3..6),
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
        assert!(
            off_by(&times[0], &playlist) <= AUDIO_FRAME_MS,
            "{times:?} {playlist:?}"
        );
        assert!(
            off_by(&times[1], &playlist[3..]) <= AUDIO_FRAME_MS,
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
