use crate::HdrType;
use crate::MediaFile;
use crate::VideoTranscode;
use uuid::Uuid;

const MAX_TRANSCODE_WIDTH: i64 = 1920;
const MAX_TRANSCODE_HEIGHT: i64 = 1080;
const MAX_TRANSCODE_BIT_RATE: i64 = 10_000_000;

#[derive(Debug, toasty::Model)]
pub struct VideoStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: i64,
    pub height: i64,
    pub frame_rate: Option<f64>,
    pub hdr_type: HdrType,
    pub interlaced: bool,
    pub bit_rate: Option<i64>,
}

impl VideoStream {
    pub fn transcode(&self) -> VideoTranscode {
        let scale = (MAX_TRANSCODE_WIDTH as f64 / self.width as f64)
            .min(MAX_TRANSCODE_HEIGHT as f64 / self.height as f64)
            .min(1.0);
        let even = |x: i64| (x as f64 * scale / 2.0).round() as i64 * 2;
        VideoTranscode {
            width: even(self.width),
            height: even(self.height),
            bit_rate: self
                .bit_rate
                .map_or(MAX_TRANSCODE_BIT_RATE, |x| x.min(MAX_TRANSCODE_BIT_RATE)),
            tone_map: self.hdr_type != HdrType::Sdr,
            deinterlace: self.interlaced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    fn video(width: i64, height: i64, bit_rate: Option<i64>) -> VideoStream {
        VideoStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 0,
            codec: "hevc".to_owned(),
            profile: Some("Main 10".to_owned()),
            width,
            height,
            frame_rate: Some(24000.0 / 1001.0),
            hdr_type: HdrType::Sdr,
            interlaced: false,
            bit_rate,
        }
    }

    #[test]
    fn a_4k_video_transcodes_to_1080p_at_most_keeping_its_shape_and_at_10_mbps_at_most() {
        let size = |width, height| {
            let target = video(width, height, Some(40_000_000)).transcode();
            (target.width, target.height, target.bit_rate)
        };

        assert_eq!(size(3840, 2160), (1920, 1080, 10_000_000));
        assert_eq!(size(3840, 1600), (1920, 800, 10_000_000));
        assert_eq!(size(2880, 2160), (1440, 1080, 10_000_000));
        assert_eq!(size(4096, 1716), (1920, 804, 10_000_000));
    }

    #[test]
    fn a_video_within_1080p_and_10_mbps_keeps_its_size_and_bitrate() {
        let target = video(720, 480, Some(5_982_144)).transcode();

        assert_eq!(
            (target.width, target.height, target.bit_rate),
            (720, 480, 5_982_144)
        );
    }

    #[test]
    fn a_video_without_a_known_bitrate_transcodes_at_10_mbps() {
        assert_eq!(video(1920, 1080, None).transcode().bit_rate, 10_000_000);
    }

    #[test]
    fn hdr_is_tone_mapped_and_interlaced_video_is_deinterlaced() {
        let target = |hdr_type, interlaced| {
            let target = VideoStream {
                hdr_type,
                interlaced,
                ..video(1920, 1080, None)
            }
            .transcode();
            (target.tone_map, target.deinterlace)
        };

        assert_eq!(target(HdrType::Sdr, false), (false, false));
        assert_eq!(target(HdrType::Sdr, true), (false, true));
        assert_eq!(target(HdrType::Hdr10, false), (true, false));
        assert_eq!(target(HdrType::Hlg, false), (true, false));
        assert_eq!(target(HdrType::DolbyVision, false), (true, false));
    }
}
