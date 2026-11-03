#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum PlayMethod {
    DirectPlay,
    Remux,
    Transcode,
}
