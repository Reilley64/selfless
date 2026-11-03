use crate::queue_consumer::QueuePauses;
use crate::queue_consumer::consume_queue;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbSet;
use domain::Inbox;
use domain::Source;
use std::future::Future;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to handle an inbox delivery")]
pub struct HandleInboxError {
    core: ohno::OhnoCore,
}

impl HandleInboxError {
    pub fn caused_by(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

pub async fn consume_inbox<F>(
    db_context: DbContext,
    handle_inbox: impl Fn(Uuid, Source, Vec<u8>) -> F,
    pauses: QueuePauses,
    cancellation: CancellationToken,
) where
    F: Future<Output = Result<(), HandleInboxError>> + Send + 'static,
{
    consume_queue(
        db_context,
        "inboxes",
        load,
        |inbox: Inbox| handle_inbox(inbox.id, inbox.source, inbox.body),
        pauses,
        cancellation,
    )
    .await;
}

fn load(
    executor: &mut dyn toasty::Executor,
    id: Uuid,
) -> Pin<Box<dyn Future<Output = toasty::Result<Option<Inbox>>> + Send + '_>> {
    Box::pin(DbSet::<Inbox>::new().find(id).exec(executor))
}
