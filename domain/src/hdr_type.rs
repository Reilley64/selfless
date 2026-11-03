#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum HdrType {
    Sdr,
    Hdr10,
    Hlg,
    DolbyVision,
}
