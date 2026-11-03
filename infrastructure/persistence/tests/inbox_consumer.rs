#[path = "../../../test/persistence.rs"]
mod persistence_test;

use application::ports::persistence::DbScope;
use domain::Source;
use persistence::QueuePauses;
use persistence::consume_inbox;
use persistence_test::db_context;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use test_fixtures::TestDatabase;
use tokio_util::sync::CancellationToken;

const PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(5),
    after_failure: Duration::from_millis(5),
};

#[tokio::test]
async fn a_stored_delivery_reaches_the_handler_with_its_source_and_body_and_is_delivered() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    let stored = db_context
        .inboxes()
        .create()
        .source(Source::Sonarr)
        .body(br#"{"eventType":"Test"}"#.to_vec())
        .received_at(jiff::Timestamp::now())
        .traceparent(None::<String>)
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    let handled = Arc::new(Mutex::new(Vec::new()));
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_inbox(
        db_context.clone(),
        {
            let handled = handled.clone();
            move |id, source, body| {
                handled.lock().unwrap().push((id, source, body));
                async { Ok(()) }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    let delivered_at = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let inbox = db_context
                .inboxes()
                .find(stored.id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .unwrap();
            if let Some(delivered_at) = inbox.delivered_at {
                return delivered_at;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the delivery is delivered");
    cancellation.cancel();
    consumer.await.unwrap();

    assert!(delivered_at >= stored.created_at);
    assert_eq!(
        handled.lock().unwrap().clone(),
        [(
            stored.id,
            Source::Sonarr,
            br#"{"eventType":"Test"}"#.to_vec()
        )]
    );
    database.drop().await;
}
