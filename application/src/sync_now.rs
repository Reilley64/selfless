use crate::Reconciles;

#[fundle::deps]
pub struct Dependencies {
    pub reconciles: Reconciles,
}

pub fn call(dependencies: impl Into<Dependencies>) {
    dependencies.into().reconciles.request();
}
