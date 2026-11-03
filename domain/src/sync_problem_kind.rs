#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum SyncProblemKind {
    UnreadableNfo,
    FileNotFound,
    FfprobeFailed,
}
