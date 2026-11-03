use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::metrics::SELFLESS_WEBHOOK_DELIVERIES;
use crate::ports::UnreadableWebhook;
use domain::Source;
use opentelemetry::KeyValue;
use opentelemetry::global;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Stored,
    Rejected,
    Processed,
    Ignored,
    Skipped,
    Failed,
}

impl Outcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Outcome::Stored => "stored",
            Outcome::Rejected => "rejected",
            Outcome::Processed => "processed",
            Outcome::Ignored => "ignored",
            Outcome::Skipped => "skipped",
            Outcome::Failed => "failed",
        }
    }
}

pub(crate) async fn process<D, E, F>(
    source: Source,
    id: Uuid,
    event_type: Option<String>,
    delivery: Result<D, UnreadableWebhook>,
    sync: impl FnOnce(D) -> F,
) -> Result<(), E>
where
    F: Future<Output = Result<Outcome, E>>,
{
    let span = Span::current();
    span.set_attribute(SELFLESS_WEBHOOK_SOURCE, source.name());
    if let Some(event_type) = &event_type {
        span.set_attribute(SELFLESS_WEBHOOK_EVENT_TYPE, event_type.clone());
    }
    let synced = match delivery {
        Ok(delivery) => sync(delivery).await,
        Err(error) => {
            tracing::warn!(
                messaging.message.id = %id,
                selfless.webhook.source = source.name(),
                error = &error as &dyn std::error::Error,
                "skipped a webhook delivery selfless cannot read"
            );
            Ok(Outcome::Skipped)
        }
    };
    let outcome = synced.as_ref().map_or(Outcome::Failed, |x| *x);
    span.set_attribute(SELFLESS_WEBHOOK_OUTCOME, outcome.name());
    count(source, event_type, outcome);
    synced.map(|_| ())
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
