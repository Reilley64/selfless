use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use domain::OutboxData;
use persistence::DatabaseSettings;
use persistence::HandleOutboxError;
use persistence::QueuePauses;
use persistence::consume_outbox;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use test_fixtures::TestDatabase;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(5),
    after_failure: Duration::from_millis(5),
};

async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}

async fn enqueue(db_context: &DbContext, count: usize) -> Vec<Uuid> {
    let mut db_context = db_context.clone();
    let mut invite_ids = Vec::new();
    for _ in 0..count {
        let invite_id = Uuid::new_v4();
        db_context
            .outboxes()
            .create()
            .data(OutboxData::SendInviteEmail {
                invite_id,
                origin: "https://selfless.example".to_owned(),
            })
            .traceparent(None::<String>)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        invite_ids.push(invite_id);
    }
    invite_ids
}

async fn undelivered(db_context: &DbContext) -> Vec<Uuid> {
    let mut db_context = db_context.clone();
    db_context
        .outboxes()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .filter(|x| x.delivered_at.is_none())
        .map(|x| invite_id(&x.data))
        .collect()
}

async fn wait_until_delivered(db_context: &DbContext) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !undelivered(db_context).await.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("every row is delivered");
}

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    fn capture(&self) -> tracing::subscriber::DefaultGuard {
        let logs = self.clone();
        tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || logs.clone())
                .finish(),
        )
    }

    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Logs {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn invite_id(data: &OutboxData) -> Uuid {
    let OutboxData::SendInviteEmail { invite_id, .. } = data;
    *invite_id
}

#[tokio::test(flavor = "multi_thread")]
async fn two_consumers_handle_each_row_once() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let mut invite_ids = enqueue(&db_context, 20).await;
    let handled = Arc::new(Mutex::new(Vec::new()));
    let cancellation = CancellationToken::new();
    let consumers = [0, 1].map(|consumer| {
        let handled = handled.clone();
        tokio::spawn(consume_outbox(
            db_context.clone(),
            move |data| {
                let handled = handled.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    handled.lock().unwrap().push((consumer, invite_id(&data)));
                    Ok(())
                }
            },
            PAUSES,
            cancellation.clone(),
        ))
    });

    wait_until_delivered(&db_context).await;
    cancellation.cancel();
    for consumer in consumers {
        consumer.await.unwrap();
    }

    let handled = handled.lock().unwrap().clone();
    let mut handled_ids = handled.iter().map(|x| x.1).collect::<Vec<_>>();
    handled_ids.sort();
    invite_ids.sort();
    assert_eq!(handled_ids, invite_ids);
    for consumer in [0, 1] {
        assert!(handled.iter().any(|x| x.0 == consumer), "{handled:?}");
    }
    drop(db_context);
    database.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_row_is_retried_after_the_rows_behind_it() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let invite_ids = enqueue(&db_context, 3).await;
    let handled = Arc::new(Mutex::new(Vec::new()));
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        {
            let handled = handled.clone();
            let failing = invite_ids[0];
            move |data| {
                let handled = handled.clone();
                async move {
                    let mut handled = handled.lock().unwrap();
                    let invite_id = invite_id(&data);
                    let first_attempt = !handled.contains(&invite_id);
                    handled.push(invite_id);
                    if invite_id == failing && first_attempt {
                        return Err(HandleOutboxError::new());
                    }
                    Ok(())
                }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    wait_until_delivered(&db_context).await;
    cancellation.cancel();
    consumer.await.unwrap();

    assert_eq!(
        *handled.lock().unwrap(),
        [invite_ids[0], invite_ids[1], invite_ids[2], invite_ids[0]]
    );
    drop(db_context);
    database.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_panicking_handler_leaves_its_row_for_a_retry_and_the_loop_goes_on() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let invite_ids = enqueue(&db_context, 2).await;
    let handled = Arc::new(Mutex::new(Vec::new()));
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        {
            let handled = handled.clone();
            let panicking = invite_ids[0];
            move |data| {
                let handled = handled.clone();
                async move {
                    let invite_id = invite_id(&data);
                    let first_attempt = !handled.lock().unwrap().contains(&invite_id);
                    handled.lock().unwrap().push(invite_id);
                    assert!(
                        invite_id != panicking || !first_attempt,
                        "the handler panics"
                    );
                    Ok(())
                }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    wait_until_delivered(&db_context).await;
    cancellation.cancel();
    consumer.await.unwrap();

    assert_eq!(
        *handled.lock().unwrap(),
        [invite_ids[0], invite_ids[1], invite_ids[0]]
    );
    drop(db_context);
    database.drop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_stops_the_loop() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let invite_ids = enqueue(&db_context, 2).await;
    let handled = Arc::new(Mutex::new(Vec::new()));
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        {
            let handled = handled.clone();
            let started = started.clone();
            let release = release.clone();
            move |data| {
                let handled = handled.clone();
                let started = started.clone();
                let release = release.clone();
                async move {
                    started.notify_one();
                    release.notified().await;
                    handled.lock().unwrap().push(invite_id(&data));
                    Ok(())
                }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    started.notified().await;
    cancellation.cancel();
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), consumer)
        .await
        .expect("the loop stops")
        .unwrap();

    assert_eq!(*handled.lock().unwrap(), [invite_ids[0]]);
    assert_eq!(undelivered(&db_context).await, [invite_ids[1]]);
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn a_database_error_restarts_the_loop() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    let logs = Logs::default();
    let _logging = logs.capture();
    toasty::sql::statement("ALTER TABLE outboxes RENAME TO outboxes_moved")
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        |_| async { Ok(()) },
        PAUSES,
        cancellation.clone(),
    ));

    tokio::time::timeout(Duration::from_secs(5), async {
        while !logs.text().contains("failed to consume a queue") {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the loop logs the failure");
    toasty::sql::statement("ALTER TABLE outboxes_moved RENAME TO outboxes")
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    enqueue(&db_context, 1).await;
    wait_until_delivered(&db_context).await;
    cancellation.cancel();
    consumer.await.unwrap();

    drop(db_context);
    database.drop().await;
}
