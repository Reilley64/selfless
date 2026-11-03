#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum SyncProblemKind {
    UnreadableNfo,
    MissingMediaFile,
    UnrecognisedMediaFile,
    UnreadableFile,
}
