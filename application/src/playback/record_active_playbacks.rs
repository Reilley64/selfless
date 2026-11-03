mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_PLAYBACK_METHOD;
use crate::metrics::SELFLESS_PLAYBACK_ACTIVE;
use crate::ports::persistence::DbScope;
use crate::telemetry::attribute_values::play_method;
use domain::PlayMethod;
use domain::Playback;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Gauge;
use std::sync::LazyLock;

static ACTIVE: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    global::meter("selfless")
        .i64_gauge(SELFLESS_PLAYBACK_ACTIVE)
        .with_unit("{playback}")
        .build()
});

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies { mut db_context } = dependencies.into();
    let open = db_context
        .playbacks()
        .filter(Playback::open_filter())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    for method in [
        PlayMethod::DirectPlay,
        PlayMethod::Remux,
        PlayMethod::Transcode,
    ] {
        let count = open.iter().filter(|x| x.method == method).count();
        ACTIVE.record(
            i64::try_from(count).unwrap_or(i64::MAX),
            &[KeyValue::new(SELFLESS_PLAYBACK_METHOD, play_method(method))],
        );
    }
    Ok(Output)
}
