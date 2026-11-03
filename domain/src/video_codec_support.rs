use crate::HdrType;
use crate::PlayReason;
use crate::VideoStream;

#[derive(Clone, Debug)]
pub struct VideoCodecSupport {
    pub codec: String,
    pub profiles: Vec<String>,
    pub max_level: i64,
    pub max_width: i64,
    pub max_height: i64,
    pub bit_depths: Vec<i64>,
    pub hdr_types: Vec<HdrType>,
    pub dolby_vision_profiles: Vec<i64>,
}

impl VideoCodecSupport {
    pub(crate) fn reasons(&self, video: &VideoStream) -> Vec<PlayReason> {
        let mut reasons = Vec::new();
        if let Some(profile) = &video.profile {
            let name = profile
                .chars()
                .filter(|x| !x.is_whitespace())
                .collect::<String>()
                .to_lowercase();
            if !self.profiles.contains(&name) {
                reasons.push(PlayReason::VideoProfile(name));
            }
        }
        if let Some(level) = video.level.filter(|x| *x > self.max_level) {
            reasons.push(PlayReason::VideoLevel(level));
        }
        if video.width > self.max_width || video.height > self.max_height {
            reasons.push(PlayReason::VideoSize {
                width: video.width,
                height: video.height,
            });
        }
        if let Some(bit_depth) = video.bit_depth.filter(|x| !self.bit_depths.contains(x)) {
            reasons.push(PlayReason::VideoBitDepth(bit_depth));
        }
        if !self.plays_range(video) {
            reasons.push(PlayReason::VideoRange(video.hdr_type));
        }
        reasons
    }

    fn plays_range(&self, video: &VideoStream) -> bool {
        match video.hdr_type {
            HdrType::DolbyVision => {
                let plays_dolby_vision = self.hdr_types.contains(&HdrType::DolbyVision)
                    && video
                        .dolby_vision_profile
                        .is_some_and(|x| self.dolby_vision_profiles.contains(&x));
                plays_dolby_vision
                    || video.has_hdr10_base_layer() && self.hdr_types.contains(&HdrType::Hdr10)
            }
            hdr_type => self.hdr_types.contains(&hdr_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;
    use uuid::Uuid;

    fn hevc(hdr_types: Vec<HdrType>, dolby_vision_profiles: Vec<i64>) -> VideoCodecSupport {
        VideoCodecSupport {
            codec: "hevc".to_owned(),
            profiles: vec!["main".to_owned(), "main10".to_owned()],
            max_level: 153,
            max_width: 3840,
            max_height: 2160,
            bit_depths: vec![8, 10],
            hdr_types,
            dolby_vision_profiles,
        }
    }

    fn video(level: i64, bit_depth: i64) -> VideoStream {
        VideoStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 0,
            codec: "hevc".to_owned(),
            profile: Some("Main 10".to_owned()),
            level: Some(level),
            bit_depth: Some(bit_depth),
            width: 3840,
            height: 1600,
            frame_rate: None,
            hdr_type: HdrType::Sdr,
            dolby_vision_profile: None,
            dolby_vision_compatibility_id: None,
            interlaced: false,
            bit_rate: None,
        }
    }

    fn dolby_vision(profile: i64, compatibility_id: i64) -> VideoStream {
        VideoStream {
            hdr_type: HdrType::DolbyVision,
            dolby_vision_profile: Some(profile),
            dolby_vision_compatibility_id: Some(compatibility_id),
            ..video(153, 10)
        }
    }

    #[test]
    fn a_level_above_the_maximum_or_a_bit_depth_the_codec_does_not_list_is_a_reason() {
        let support = hevc(vec![HdrType::Sdr], Vec::new());

        let reasons = [video(153, 10), video(156, 10), video(153, 12)].map(|x| support.reasons(&x));

        assert_eq!(
            reasons,
            [
                vec![],
                vec![PlayReason::VideoLevel(156)],
                vec![PlayReason::VideoBitDepth(12)],
            ]
        );
    }

    #[test]
    fn dolby_vision_plays_as_dolby_vision_only_in_a_profile_the_codec_lists() {
        let support = hevc(vec![HdrType::Sdr, HdrType::DolbyVision], vec![8]);

        let reasons = [dolby_vision(8, 1), dolby_vision(5, 0)].map(|x| support.reasons(&x));

        assert_eq!(
            reasons,
            [vec![], vec![PlayReason::VideoRange(HdrType::DolbyVision)]]
        );
    }

    #[test]
    fn dolby_vision_with_an_hdr10_base_layer_plays_as_hdr10_where_hdr10_plays() {
        let hdr10 = hevc(vec![HdrType::Sdr, HdrType::Hdr10], Vec::new());
        let sdr = hevc(vec![HdrType::Sdr], Vec::new());

        let reasons = [
            hdr10.reasons(&dolby_vision(8, 1)),
            hdr10.reasons(&dolby_vision(7, 6)),
            hdr10.reasons(&dolby_vision(5, 0)),
            sdr.reasons(&dolby_vision(8, 1)),
        ];

        let range = vec![PlayReason::VideoRange(HdrType::DolbyVision)];
        assert_eq!(reasons, [vec![], vec![], range.clone(), range]);
    }
}
