mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

pub fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    dependencies.into().reconciles.request();
    Ok(Output)
}
