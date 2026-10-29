#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum ClientKind {
    Web,
    AppleTv,
    GoogleTv,
}
