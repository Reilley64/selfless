use application::ports::AudioStream;
use application::ports::VideoStream;
use domain::VideoTranscode;
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

    pub fn media_playlist(&self) -> String {
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
             #EXT-X-MAP:URI=\"{INIT}\"\n"
        );
        for (index, length) in lengths.iter().enumerate() {
            playlist.push_str(&format!(
                "#EXTINF:{:.6},\n{index}.m4s\n",
                length.as_secs_f64()
            ));
        }
        playlist.push_str("#EXT-X-ENDLIST\n");
        playlist
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn write_segments(
    input: &Path,
    video: &VideoStream,
    video_transcode: Option<&VideoTranscode>,
    audio: Option<&AudioStream>,
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
    if let Some(audio) = audio {
        command.args(["-map", &format!("0:{}", audio.index)]);
    }
    command.args(["-c", "copy"]);
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
        None if video.codec == "hevc" => {
            command.args(["-tag:v", "hvc1"]);
        }
        None => {}
    }
    let mut child = command
        .args(["-copypriorss", "0", "-avoid_negative_ts", "disabled"])
        .args(["-f", "segment", "-segment_format", "mp4"])
        .args(["-segment_format_options", SEGMENT_FORMAT_OPTIONS])
        .args(["-segment_times", &cuts, "-segment_time_delta", "0.001"])
        .args(["-segment_start_number", &segment.to_string()])
        .args(["-segment_list", "pipe:1", "-segment_list_type", "flat"])
        .arg(folder.join("%d.mp4"))
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
    let mut last = None;
    while let Some(next) = finished.next_line().await? {
        if let Some(name) = last.replace(next) {
            written.send_replace(split_segment(folder, &name).await? + 1);
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
    if let Some(name) = last {
        written.send_replace(split_segment(folder, &name).await? + 1);
    }
    Ok(())
}

async fn split_segment(folder: &Path, name: &str) -> io::Result<usize> {
    let index = name
        .strip_suffix(".mp4")
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected segment name"))?;
    let path = folder.join(name);
    let bytes = fs::read(&path).await?;
    let header = header_length(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "segment without moov"))?;
    let init = folder.join(INIT);
    if !fs::try_exists(&init).await? {
        write_atomically(&init, &bytes[..header]).await?;
    }
    write_atomically(&path.with_extension("m4s"), &bytes[header..]).await?;
    fs::remove_file(path).await?;
    Ok(index)
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

fn gop(frame_rate: f64) -> u64 {
    (SEGMENT_MARK.as_secs_f64() * frame_rate).round() as u64
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
    use application::ports::VideoStream;
    use domain::HdrType;
    use mp4_atom::Any;
    use mp4_atom::Decode;
    use mp4_atom::FourCC;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::TempDir;

    const FIXTURE_DURATION: Duration = Duration::from_secs(40);

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
            segments.media_playlist(),
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
            playlist_times(&Segments::fixed(frame_rate, Duration::from_secs(20)).media_playlist())
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

    fn playlist_times(playlist: &str) -> Vec<(u64, u64)> {
        let mut start = 0;
        playlist
            .lines()
            .filter_map(|x| x.strip_prefix("#EXTINF:")?.strip_suffix(','))
            .map(|x| {
                let end = start + (x.parse::<f64>().unwrap() * 1000.0).round() as u64;
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

    fn video_segment_times(
        folder: &Path,
        segments: impl Iterator<Item = usize>,
    ) -> Vec<(u64, u64)> {
        let (track_id, timescale) = atoms(&folder.join("init.mp4"))
            .into_iter()
            .find_map(|x| match x {
                Any::Moov(moov) => moov
                    .trak
                    .iter()
                    .find(|x| x.mdia.hdlr.handler == FourCC::new(b"vide"))
                    .map(|x| (x.tkhd.track_id, u64::from(x.mdia.mdhd.timescale))),
                _ => None,
            })
            .expect("the init segment has a video track");
        segments
            .map(|index| {
                let mut start = i64::MAX;
                let mut end = i64::MIN;
                for atom in atoms(&folder.join(format!("{index}.m4s"))) {
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
                let milliseconds = |x: i64| (x as u64 * 1000 + timescale / 2) / timescale;
                (milliseconds(start), milliseconds(end))
            })
            .collect()
    }

    async fn remux_fixture(name: &str, audio_codec: &str, segment: usize) -> (Segments, TempDir) {
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
            interlaced: false,
            bit_rate: None,
        };
        let audio = AudioStream {
            index: 1,
            codec: audio_codec.to_owned(),
            language: None,
            channels: 1,
            title: None,
            default: true,
        };
        write_segments(
            &input,
            &video,
            None,
            Some(&audio),
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
        let (segments, from_start) = remux_fixture(name, audio_codec, 0).await;
        let (_, restarted) = remux_fixture(name, audio_codec, 3).await;
        let playlist = playlist_times(&segments.media_playlist());

        assert_eq!(playlist.len(), 6);
        assert_eq!(video_segment_times(from_start.path(), 0..6), playlist);
        assert_eq!(video_segment_times(restarted.path(), 3..6), playlist[3..]);
        assert_eq!(
            std::fs::read(restarted.path().join("init.mp4")).unwrap(),
            std::fs::read(from_start.path().join("init.mp4")).unwrap()
        );
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
}
