#[path = "../test/persistence.rs"]
mod persistence_test;
#[path = "../test/selfless.rs"]
mod selfless_test;

use application::invites::create_invite;
use application::invites::list_pending_invites;
use application::ports::Clock;
use domain::EmailAddress;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use serde_json::json;
use std::time::Duration;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use uuid::Uuid;

const ORIGIN: &str = "https://selfless.example";
const PASSWORD: &str = "a long passphrase about tea";

#[tokio::test]
async fn an_invite_email_goes_out_while_the_server_runs() {
    let database = TestDatabase::create().await;
    let clock = FakeClock::new();
    let mailer = FakeMailer::default();
    let server =
        selfless_test::start(selfless_test::settings(database.url()), &clock, &mailer).await;
    let db_context = persistence_test::db_context(&database).await;

    create_invite::call(
        create_invite::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(clock.reader()),
        },
        Uuid::nil(),
        EmailAddress::new("noor@household.example".to_owned()).unwrap(),
        ORIGIN.to_owned(),
    )
    .await
    .unwrap();
    let email = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(email) = mailer.sent().pop() {
                return email;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the worker sends the invite email");
    tokio::time::timeout(Duration::from_secs(5), async {
        while !matches!(
            list_pending_invites::call(list_pending_invites::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            })
            .await
            .unwrap()
            .invites[0]
                .email_status,
            list_pending_invites::EmailStatus::Sent { .. }
        ) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the worker records that the email went out");
    let token = email.invite_token(ORIGIN);
    let mut headers = HeaderMap::new();
    headers.insert(
        "idempotency-key",
        Uuid::new_v4().to_string().parse().unwrap(),
    );
    let (accepted, user) = server
        .request_with_headers(
            Method::POST,
            "/api/invites/accept/v1",
            None,
            headers,
            Some(json!({ "token": token, "name": "Noor", "password": PASSWORD })),
        )
        .await;
    let (signed_in, session) = server
        .request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": "Noor",
                "password": PASSWORD,
                "clientKind": "web",
                "deviceName": "Firefox on macOS",
                "clientId": Uuid::new_v4(),
            })),
        )
        .await;

    assert_eq!(email.recipient, "noor@household.example");
    assert_eq!(accepted, StatusCode::CREATED);
    assert_eq!(signed_in, StatusCode::CREATED);
    assert_eq!(session["user"], user);
    drop(db_context);
    drop(server);
    database.drop().await;
}
