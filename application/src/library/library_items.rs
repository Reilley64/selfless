use crate::attributes::SELFLESS_LIBRARY_SOURCE;
use crate::metrics::SELFLESS_LIBRARY_ITEMS_ADDED;
use crate::metrics::SELFLESS_LIBRARY_ITEMS_CHANGED;
use crate::metrics::SELFLESS_LIBRARY_ITEMS_REMOVED;
use domain::Source;
use opentelemetry::KeyValue;
use opentelemetry::global;

pub(crate) enum Change {
    Added,
    Changed,
    Removed,
}

pub(crate) fn count(source: Source, change: Change) {
    let name = match change {
        Change::Added => SELFLESS_LIBRARY_ITEMS_ADDED,
        Change::Changed => SELFLESS_LIBRARY_ITEMS_CHANGED,
        Change::Removed => SELFLESS_LIBRARY_ITEMS_REMOVED,
    };
    global::meter("selfless")
        .u64_counter(name)
        .with_unit("{item}")
        .build()
        .add(1, &[KeyValue::new(SELFLESS_LIBRARY_SOURCE, source.name())]);
}
