#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
#[column(rename_all = "camelCase")]
pub enum ClientKind {
    Web,
    AppleTv,
    GoogleTv,
}
