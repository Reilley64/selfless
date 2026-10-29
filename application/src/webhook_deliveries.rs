use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::metrics::SELFLESS_WEBHOOK_DELIVERIES;
use domain::Source;
use opentelemetry::KeyValue;
use opentelemetry::global;

#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Stored,
    Rejected,
    Processed,
    Ignored,
    Failed,
}

impl Outcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Outcome::Stored => "stored",
            Outcome::Rejected => "rejected",
            Outcome::Processed => "processed",
            Outcome::Ignored => "ignored",
            Outcome::Failed => "failed",
        }
    }
}

pub(crate) fn count(source: Source, event_type: Option<String>, outcome: Outcome) {
    let mut attributes = vec![
        KeyValue::new(SELFLESS_WEBHOOK_SOURCE, source.name()),
        KeyValue::new(SELFLESS_WEBHOOK_OUTCOME, outcome.name()),
    ];
    if let Some(event_type) = event_type {
        attributes.push(KeyValue::new(SELFLESS_WEBHOOK_EVENT_TYPE, event_type));
    }
    global::meter("selfless")
        .u64_counter(SELFLESS_WEBHOOK_DELIVERIES)
        .with_unit("{delivery}")
        .build()
        .add(1, &attributes);
}
