use application::invites::create_invite;
use application::invites::send_invite_email;
use application::ports::Clock;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use domain::EmailAddress;
use domain::OutboxData;
use lettre::AsyncSmtpTransport;
use lettre::Tokio1Executor;
use opentelemetry::trace::SpanId;
use opentelemetry::trace::SpanKind;
use opentelemetry::trace::Status;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::trace::SpanData;
use persistence::DatabaseSettings;
use persistence::HandleOutboxError;
use persistence::QueuePauses;
use persistence::consume_outbox;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use test_fixtures::FakeClock;
use test_fixtures::ScriptedSmtpServer;
use test_fixtures::TestDatabase;
use tokio_util::sync::CancellationToken;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

const PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(5),
    after_failure: Duration::from_millis(5),
};

const TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}

fn trace(exporter: &InMemorySpanExporter) -> DefaultGuard {
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test"))),
    )
}

fn trace_and_log(exporter: &InMemorySpanExporter, logs: &Logs) -> DefaultGuard {
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let logs = logs.clone();
    tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(EnvFilter::new("info"))
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(move || logs.clone()),
            ),
    )
}

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
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

async fn enqueue(db_context: &DbContext) -> Uuid {
    let mut db_context = db_context.clone();
    db_context
        .outboxes()
        .create()
        .data(OutboxData::SendInviteEmail {
            invite_id: Uuid::new_v4(),
            origin: "https://selfless.example".to_owned(),
        })
        .traceparent(Some(TRACEPARENT.to_owned()))
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}

async fn process_spans(exporter: &InMemorySpanExporter, count: usize) -> Vec<SpanData> {
    let spans = || {
        exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| x.name == "process outboxes")
            .collect::<Vec<_>>()
    };
    tokio::time::timeout(Duration::from_secs(20), async {
        while spans().len() < count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the rows are processed");
    spans()
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|x| x.key.as_str() == key)
        .map(|x| x.value.to_string())
}

#[tokio::test]
async fn a_processed_row_exports_one_consumer_span_linked_to_its_traceparent() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let exporter = InMemorySpanExporter::default();
    let _tracing = trace(&exporter);
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        |_| async {
            tracing::info_span!("handler").in_scope(|| ());
            Ok(())
        },
        PAUSES,
        cancellation.clone(),
    ));

    exporter.reset();
    let id = enqueue(&db_context).await;
    process_spans(&exporter, 1).await;
    cancellation.cancel();
    consumer.await.unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    let [process] = spans
        .iter()
        .filter(|x| x.name == "process outboxes")
        .collect::<Vec<_>>()[..]
    else {
        panic!("one process span: {spans:#?}");
    };
    assert_eq!(process.span_kind, SpanKind::Consumer);
    assert_eq!(process.parent_span_id, SpanId::INVALID);
    assert_eq!(
        attribute(process, "messaging.destination.name").as_deref(),
        Some("outboxes")
    );
    assert_eq!(
        attribute(process, "messaging.message.id"),
        Some(id.to_string())
    );
    let links = process
        .links
        .iter()
        .map(|x| {
            (
                x.span_context.trace_id().to_string(),
                x.span_context.span_id().to_string(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        links,
        [(
            "0af7651916cd43dd8448eb211c80319c".to_owned(),
            "b7ad6b7169203331".to_owned()
        )]
    );
    let handler = spans.iter().find(|x| x.name == "handler").unwrap();
    assert_eq!(handler.parent_span_id, process.span_context.span_id());
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn a_failed_row_records_an_error_on_its_span() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let exporter = InMemorySpanExporter::default();
    let _tracing = trace(&exporter);
    let attempts = Arc::new(AtomicUsize::new(0));
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        move |_| {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                match attempt {
                    0 => Err(HandleOutboxError::new()),
                    1 => panic!("the handler panics"),
                    _ => Ok(()),
                }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    exporter.reset();
    enqueue(&db_context).await;
    let spans = process_spans(&exporter, 3).await;
    cancellation.cancel();
    consumer.await.unwrap();

    let outcomes = spans
        .iter()
        .map(|x| (x.status.clone(), attribute(x, "error.type")))
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            (Status::error(""), Some("handler_error".to_owned())),
            (Status::error(""), Some("handler_panic".to_owned())),
            (Status::Unset, None),
        ]
    );
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn an_idle_loop_exports_no_spans() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let exporter = InMemorySpanExporter::default();
    let _tracing = trace(&exporter);
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        |_| async { Ok(()) },
        PAUSES,
        cancellation.clone(),
    ));

    tokio::time::sleep(PAUSES.idle * 20).await;
    cancellation.cancel();
    consumer.await.unwrap();

    let spans = exporter
        .get_finished_spans()
        .unwrap()
        .into_iter()
        .map(|x| x.name)
        .collect::<Vec<_>>();
    assert_eq!(spans, Vec::<&str>::new());
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn a_failed_send_logs_no_address() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    let address = "noor@household.example";
    let smtp =
        ScriptedSmtpServer::refusing("RCPT", format!("451 4.3.0 <{address}> try later")).await;
    let clock = FakeClock::new();
    let acquire_lock = locking::acquire_lock();
    let send_email = email::send_email_through(
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
            .port(smtp.port())
            .build(),
        "selfless@household.example".parse().unwrap(),
    );
    let exporter = InMemorySpanExporter::default();
    let logs = Logs::default();
    let _tracing = trace_and_log(&exporter, &logs);
    let cancellation = CancellationToken::new();
    let consumer = tokio::spawn(consume_outbox(
        db_context.clone(),
        {
            let db_context = db_context.clone();
            let clock = Clock::new(clock.reader());
            move |data| {
                let dependencies = send_invite_email::Dependencies {
                    db_context: db_context.clone(),
                    clock: clock.clone(),
                    acquire_lock: acquire_lock.clone(),
                    send_email: send_email.clone(),
                };
                async move {
                    let OutboxData::SendInviteEmail { invite_id, origin } = data;
                    send_invite_email::call(dependencies, invite_id, origin)
                        .await
                        .map_err(HandleOutboxError::caused_by)?;
                    Ok(())
                }
            }
        },
        PAUSES,
        cancellation.clone(),
    ));

    exporter.reset();
    create_invite::call(
        create_invite::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(clock.reader()),
        },
        Uuid::nil(),
        EmailAddress::new(address.to_owned()).unwrap(),
        "https://selfless.example".to_owned(),
    )
    .await
    .unwrap();
    process_spans(&exporter, 1).await;
    cancellation.cancel();
    consumer.await.unwrap();

    let logs = logs.text();
    let spans = format!("{:?}", exporter.get_finished_spans().unwrap());
    assert!(logs.contains("failed to handle a queue item"), "{logs}");
    assert!(logs.contains("451"), "{logs}");
    for text in [address, "try later"] {
        assert!(!logs.contains(text), "{logs}");
        assert!(!spans.contains(text), "{spans}");
    }
    drop(db_context);
    database.drop().await;
}
