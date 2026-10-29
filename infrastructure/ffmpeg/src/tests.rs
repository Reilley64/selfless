use super::parse;
use application::ports::AudioStream;
use application::ports::HdrType;
use application::ports::Probe;
use application::ports::SubtitleStream;
use application::ports::VideoStream;
use std::time::Duration;

fn subtitle(index: i64, codec: &str, language: &str, title: Option<&str>) -> SubtitleStream {
    SubtitleStream {
        index,
        codec: codec.to_owned(),
        language: Some(language.to_owned()),
        title: title.map(str::to_owned),
        default: false,
        forced: false,
        hearing_impaired: false,
        text: true,
    }
}

#[test]
fn an_hdr10_matroska_movie_gives_every_stream_and_skips_the_cover_art() {
    let probe = parse(include_bytes!("../fixtures/hdr10-movie.mkv.json")).unwrap();

    assert_eq!(
        probe,
        Probe {
            container: "matroska,webm".to_owned(),
            duration: Some(Duration::from_millis(2021)),
            video_streams: vec![VideoStream {
                index: 0,
                codec: "hevc".to_owned(),
                profile: Some("Main 10".to_owned()),
                width: 320,
                height: 134,
                frame_rate: Some(24000.0 / 1001.0),
                hdr_type: HdrType::Hdr10,
            }],
            audio_streams: vec![
                AudioStream {
                    index: 1,
                    codec: "eac3".to_owned(),
                    language: Some("eng".to_owned()),
                    channels: 6,
                    title: Some("Surround 5.1".to_owned()),
                    default: true,
                },
                AudioStream {
                    index: 2,
                    codec: "aac".to_owned(),
                    language: Some("eng".to_owned()),
                    channels: 2,
                    title: Some("Commentary".to_owned()),
                    default: false,
                },
            ],
            subtitle_streams: vec![
                SubtitleStream {
                    default: true,
                    ..subtitle(3, "subrip", "eng", Some("English"))
                },
                SubtitleStream {
                    forced: true,
                    ..subtitle(4, "subrip", "eng", Some("Forced"))
                },
                SubtitleStream {
                    hearing_impaired: true,
                    ..subtitle(5, "subrip", "eng", Some("SDH"))
                },
                subtitle(6, "ass", "jpn", None),
                SubtitleStream {
                    text: false,
                    ..subtitle(7, "hdmv_pgs_subtitle", "fre", None)
                },
            ],
        }
    );
}

#[test]
fn an_sdr_mp4_episode_with_a_text_subtitle_has_the_sdr_hdr_type() {
    let probe = parse(include_bytes!("../fixtures/sdr-episode.mp4.json")).unwrap();

    assert_eq!(
        probe,
        Probe {
            container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
            duration: Some(Duration::from_secs(2)),
            video_streams: vec![VideoStream {
                index: 0,
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                width: 320,
                height: 180,
                frame_rate: Some(25.0),
                hdr_type: HdrType::Sdr,
            }],
            audio_streams: vec![AudioStream {
                index: 1,
                codec: "aac".to_owned(),
                language: Some("eng".to_owned()),
                channels: 2,
                title: None,
                default: true,
            }],
            subtitle_streams: vec![SubtitleStream {
                default: true,
                ..subtitle(2, "mov_text", "eng", None)
            }],
        }
    );
}

#[test]
fn an_hlg_video_has_the_hlg_hdr_type_and_audio_without_a_language() {
    let probe = parse(include_bytes!("../fixtures/hlg.mkv.json")).unwrap();

    assert_eq!(probe.video_streams[0].hdr_type, HdrType::Hlg);
    assert_eq!(probe.video_streams[0].frame_rate, Some(50.0));
    assert_eq!(probe.audio_streams[0].language, None);
}

#[test]
fn a_dolby_vision_video_has_the_dolby_vision_hdr_type() {
    let probe = parse(include_bytes!("../fixtures/dolby-vision.mp4.json")).unwrap();

    assert_eq!(probe.video_streams[0].hdr_type, HdrType::DolbyVision);
    assert_eq!(probe.video_streams[0].width, 1920);
    assert_eq!(probe.video_streams[0].height, 1080);
}
