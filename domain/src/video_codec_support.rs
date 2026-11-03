use crate::HdrType;
use crate::PlayReason;
use crate::VideoStream;

#[derive(Clone, Debug)]
pub struct VideoCodecSupport {
    pub codec: String,
    pub profiles: Vec<String>,
    pub max_width: i64,
    pub max_height: i64,
    pub hdr_types: Vec<HdrType>,
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
        if video.width > self.max_width || video.height > self.max_height {
            reasons.push(PlayReason::VideoSize {
                width: video.width,
                height: video.height,
            });
        }
        let base_layer_plays =
            video.hdr_type == HdrType::DolbyVision && self.hdr_types.contains(&HdrType::Hdr10);
        if !self.hdr_types.contains(&video.hdr_type) && !base_layer_plays {
            reasons.push(PlayReason::VideoRange(video.hdr_type));
        }
        reasons
    }
}
