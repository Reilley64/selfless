#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoTranscode {
    pub width: i64,
    pub height: i64,
    pub bit_rate: i64,
    pub tone_map: bool,
    pub deinterlace: bool,
}
