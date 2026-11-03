mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    dependencies.into().reconciles.requested().await;
    Ok(Output)
}
