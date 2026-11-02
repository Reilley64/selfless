use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::trace_context::span_context;
use opentelemetry::Context;
use opentelemetry::trace::SpanContext;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::trace::TraceFlags;
use opentelemetry::trace::TraceState;
use opentelemetry_sdk::trace::IdGenerator;
use opentelemetry_sdk::trace::RandomIdGenerator;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use toasty::stmt::Type;
use toasty::stmt::Value;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::Span;
use tracing::field;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct QueuePauses {
    pub idle: Duration,
    pub after_failure: Duration,
}

pub(crate) type LoadQueueItem<T> =
    for<'a> fn(
        &'a mut dyn toasty::Executor,
        Uuid,
    ) -> Pin<Box<dyn Future<Output = toasty::Result<Option<T>>> + Send + 'a>>;

#[ohno::error]
#[display("failed to consume a queue")]
#[from(toasty::Error)]
struct ConsumeQueueError;

enum Claim {
    Empty,
    Delivered,
    Failed(Uuid),
}

pub(crate) async fn consume_queue<T, F, E>(
    mut db_context: DbContext,
    table: &'static str,
    load: LoadQueueItem<T>,
    handle: impl Fn(T) -> F,
    pauses: QueuePauses,
    cancellation: CancellationToken,
) where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Error + Send + Sync + 'static,
{
    loop {
        let pause = match claim(&mut db_context, table, load, &handle).await {
            Ok(Claim::Empty) => pauses.idle,
            Ok(Claim::Delivered) => Duration::ZERO,
            Ok(Claim::Failed(id)) => {
                if let Err(error) = toasty::sql::statement(format!(
                    "UPDATE {table} SET updated_at = statement_timestamp() WHERE id = $1"
                ))
                .bind(id)
                .exec(db_context.executor_mut())
                .await
                {
                    tracing::error!(error = &error as &dyn Error, "failed to consume a queue");
                }
                pauses.after_failure
            }
            Err(error) => {
                tracing::error!(error = &error as &dyn Error, "failed to consume a queue");
                pauses.after_failure
            }
        };
        if cancellation
            .run_until_cancelled(tokio::time::sleep(pause))
            .await
            .is_none()
        {
            return;
        }
    }
}

async fn claim<T, F, E>(
    db_context: &mut DbContext,
    table: &'static str,
    load: LoadQueueItem<T>,
    handle: &impl Fn(T) -> F,
) -> Result<Claim, ConsumeQueueError>
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Error + Send + Sync + 'static,
{
    let poll = tracing::info_span!(parent: None, "poll");
    let ids = RandomIdGenerator::default();
    let _ = poll.set_parent(Context::new().with_remote_span_context(SpanContext::new(
        ids.new_trace_id(),
        ids.new_span_id(),
        TraceFlags::NOT_SAMPLED,
        true,
        TraceState::NONE,
    )));
    let mut transaction = db_context.transaction().instrument(poll.clone()).await?;
    let rows = toasty::sql::query(format!(
        "SELECT id, traceparent FROM {table} WHERE delivered_at IS NULL \
         ORDER BY updated_at, id FOR UPDATE SKIP LOCKED LIMIT 1"
    ))
    .column_types([Type::Uuid, Type::String])
    .exec(transaction.executor_mut())
    .instrument(poll.clone())
    .await?;
    let Some(Value::Record(row)) = rows.into_iter().next() else {
        transaction.rollback().instrument(poll).await?;
        return Ok(Claim::Empty);
    };
    let [Value::Uuid(id), traceparent] = row.fields.as_slice() else {
        return Err(ConsumeQueueError::new());
    };
    let id = *id;
    let span = tracing::info_span!(
        parent: None,
        "process",
        otel.name = format!("process {table}"),
        otel.kind = "consumer",
        otel.status_code = field::Empty,
        messaging.system = "postgresql",
        messaging.operation.name = "process",
        messaging.operation.type = "process",
        messaging.destination.name = table,
        messaging.message.id = %id,
        error.type = field::Empty,
    );
    if let Value::String(traceparent) = traceparent {
        span.add_link(span_context(traceparent));
    }
    let processed = async move {
        let error_type = match load(transaction.executor_mut(), id).await? {
            Some(item) => match tokio::spawn(handle(item).in_current_span()).await {
                Ok(Ok(())) => {
                    let updated = toasty::sql::statement(format!(
                        "UPDATE {table} SET delivered_at = statement_timestamp(), \
                         updated_at = statement_timestamp() WHERE id = $1"
                    ))
                    .bind(id)
                    .exec(transaction.executor_mut())
                    .await?;
                    if updated != 1 {
                        return Err(ConsumeQueueError::new());
                    }
                    transaction.commit().await?;
                    return Ok(Claim::Delivered);
                }
                Ok(Err(error)) => {
                    tracing::error!(
                        error = &error as &dyn Error,
                        "failed to handle a queue item"
                    );
                    "handler_error"
                }
                Err(error) => {
                    tracing::error!(
                        error = &error as &dyn Error,
                        "a queue item handler panicked"
                    );
                    "handler_panic"
                }
            },
            None => {
                tracing::error!("a claimed queue item is missing");
                "item_missing"
            }
        };
        record_failure(&Span::current(), error_type);
        transaction.rollback().await?;
        Ok(Claim::Failed(id))
    }
    .instrument(span.clone())
    .await;
    if processed.is_err() {
        record_failure(&span, "database_error");
    }
    processed
}

fn record_failure(span: &Span, error_type: &'static str) {
    span.record("otel.status_code", "ERROR");
    span.record("error.type", error_type);
}
