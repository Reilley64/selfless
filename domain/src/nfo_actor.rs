#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NfoActor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}
