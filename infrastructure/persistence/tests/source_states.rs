#[path = "../../../test/persistence.rs"]
mod persistence_test;

use application::ports::Clock;
use application::ports::ReadWebhookEventType;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::webhooks::WebhookPasswords;
use application::webhooks::store_webhook_delivery;
use bytes::Bytes;
use domain::Source;
use domain::SourceState;
use persistence_test::db_context;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;

async fn reject(db_context: &DbContext, source: Source) {
    let rejected = store_webhook_delivery::call(
        store_webhook_delivery::Dependencies {
            clock: Clock::new(FakeClock::new().reader()),
            db_context: db_context.clone(),
            read_webhook_event_type: ReadWebhookEventType::new(|_| None),
            webhook_passwords: WebhookPasswords {
                radarr: "radarr webhook password".to_owned(),
                sonarr: "sonarr webhook password".to_owned(),
            },
        },
        source,
        Some("wrong password".to_owned()),
        Bytes::from_static(br#"{"eventType":"Test"}"#),
    )
    .await
    .unwrap();
    assert!(matches!(rejected, store_webhook_delivery::Output::Rejected));
}

async fn rejected_webhooks(db_context: &mut DbContext, source: Source) -> i64 {
    db_context
        .source_states()
        .filter(SourceState::fields().source().eq(source))
        .one()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .rejected_webhooks
}

#[tokio::test]
async fn each_rejected_webhook_adds_one_to_its_sources_rejected_count() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;

    reject(&db_context, Source::Radarr).await;
    reject(&db_context, Source::Radarr).await;
    reject(&db_context, Source::Sonarr).await;

    assert_eq!(rejected_webhooks(&mut db_context, Source::Radarr).await, 2);
    assert_eq!(rejected_webhooks(&mut db_context, Source::Sonarr).await, 1);
    drop(db_context);
    database.drop().await;
}
