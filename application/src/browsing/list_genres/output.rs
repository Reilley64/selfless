#[derive(Debug)]
pub struct Output {
    pub genres: Vec<Genre>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Genre {
    pub name: String,
    pub count: i64,
}
