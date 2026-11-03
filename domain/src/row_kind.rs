pub(crate) const ROW_SIZE: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    ContinueWatching,
    RecentlyAdded,
    PopularNow,
    Genre(String),
}
