use crate::queue_consumer::QueuePauses;
use crate::queue_consumer::consume_queue;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbSet;
use domain::Outbox;
use domain::OutboxData;
use std::future::Future;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to handle an outbox item")]
pub struct HandleOutboxError {
    core: ohno::OhnoCore,
}

impl HandleOutboxError {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn caused_by(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

pub async fn consume_outbox<F>(
    db_context: DbContext,
    handle_outbox: impl Fn(OutboxData) -> F,
    pauses: QueuePauses,
    cancellation: CancellationToken,
) where
    F: Future<Output = Result<(), HandleOutboxError>> + Send + 'static,
{
    consume_queue(
        db_context,
        "outboxes",
        load,
        |outbox: Outbox| handle_outbox(outbox.data),
        pauses,
        cancellation,
    )
    .await;
}

fn load(
    executor: &mut dyn toasty::Executor,
    id: Uuid,
) -> Pin<Box<dyn Future<Output = toasty::Result<Option<Outbox>>> + Send + '_>> {
    Box::pin(DbSet::<Outbox>::new().find(id).exec(executor))
}
