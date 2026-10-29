mod support;

use application::create_invite::AddressTaken;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::SignedDuration;
use jiff::Timestamp;
use ohno::ErrorExt;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use support::ADMIN_NAME;
use support::TestDatabase;
use support::TestServer;
use uuid::Uuid;

const ORIGIN: &str = "https://selfless.example";
const EMAIL: &str = "noor@household.example";
const NAME: &str = "Noor";
const PASSWORD: &str = "a long passphrase about tea";
const INVITE_NOT_FOUND: &str = "urn:selfless:problem:invite-not-found";

async fn invite(
    server: &TestServer,
    database: &TestDatabase,
    email: &str,
) -> Result<Uuid, application::create_invite::Error> {
    application::create_invite::call(
        application::create_invite::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
            send_email: server.mailer.send_email(),
        },
        database.admin_id(ADMIN_NAME).await,
        email.to_owned(),
        ORIGIN.to_owned(),
    )
    .await
}

fn token_from_last_email(server: &TestServer) -> String {
    token_from(&server.mailer.sent().last().unwrap().body)
}

fn token_from(body: &str) -> String {
    let start = body
        .find(&format!("{ORIGIN}/invite#"))
        .unwrap_or_else(|| panic!("the email has no invite link\n{body}"))
        + format!("{ORIGIN}/invite#").len();
    body[start..].split_whitespace().next().unwrap().to_owned()
}

async fn pending_invites(server: &TestServer) -> Vec<(String, Timestamp, Timestamp)> {
    application::list_pending_invites::call(application::list_pending_invites::Dependencies {
        db_context: server.db_context().await,
    })
    .await
    .unwrap()
    .into_iter()
    .map(|x| (x.email, x.sent_at, x.expires_at))
    .collect()
}

async fn invite_id(server: &TestServer, email: &str) -> Uuid {
    application::list_pending_invites::call(application::list_pending_invites::Dependencies {
        db_context: server.db_context().await,
    })
    .await
    .unwrap()
    .into_iter()
    .find(|x| x.email == email)
    .unwrap()
    .id
}

async fn resend(server: &TestServer, id: Uuid) {
    application::resend_invite::call(
        application::resend_invite::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
            send_email: server.mailer.send_email(),
        },
        id,
        ORIGIN.to_owned(),
    )
    .await
    .unwrap();
}

async fn cancel(server: &TestServer, id: Uuid) {
    application::cancel_invite::call(
        application::cancel_invite::Dependencies {
            db_context: server.db_context().await,
        },
        id,
    )
    .await
    .unwrap();
}

async fn accept(server: &TestServer, token: &str, name: &str) -> (StatusCode, Value) {
    accept_with_password(server, token, name, PASSWORD).await
}

async fn accept_with_password(
    server: &TestServer,
    token: &str,
    name: &str,
    password: &str,
) -> (StatusCode, Value) {
    server
        .request(
            Method::POST,
            "/api/invites/accept/v1",
            None,
            Some(json!({ "token": token, "name": name, "password": password })),
        )
        .await
}

#[tokio::test]
async fn an_invited_person_accepts_with_the_link_from_the_email_and_signs_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();

    let sent = server.mailer.sent();
    let (status, user) = accept(&server, &token_from_last_email(&server), NAME).await;
    let (signed_in, body) = server.sign_in(NAME, PASSWORD, "web").await;

    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].recipient, EMAIL);
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(user["name"], NAME);
    assert_eq!(signed_in, StatusCode::CREATED);
    assert_eq!(body["user"], user);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_link_holds_32_random_bytes_and_the_database_stores_only_their_hash() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    invite(&server, &database, "ravi@example.com")
        .await
        .unwrap();

    let tokens = server
        .mailer
        .sent()
        .iter()
        .map(|x| token_from(&x.body))
        .collect::<Vec<_>>();
    let stored = database
        .query("SELECT token_hash FROM invites ORDER BY sent_at, email")
        .await
        .iter()
        .map(|x| x.get::<_, Vec<u8>>("token_hash"))
        .collect::<Vec<_>>();

    assert_ne!(tokens[0], tokens[1]);
    for token in &tokens {
        assert_eq!(URL_SAFE_NO_PAD.decode(token).unwrap().len(), 32);
    }
    let mut hashes = tokens
        .iter()
        .map(|x| Sha256::digest(x.as_bytes()).to_vec())
        .collect::<Vec<_>>();
    hashes.sort();
    let mut stored = stored;
    stored.sort();
    assert_eq!(stored, hashes);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_invite_records_the_admin_who_sent_it_and_expires_7_days_later() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    invite(&server, &database, EMAIL).await.unwrap();

    let rows = database
        .query(
            "SELECT id::text, admin_id::text, extract(epoch FROM sent_at)::float8 AS sent_at, \
             extract(epoch FROM expires_at - sent_at)::float8 AS valid_for FROM invites",
        )
        .await;
    let id = rows[0].get::<_, &str>("id").parse::<Uuid>().unwrap();
    assert_eq!(id.get_version_num(), 7);
    assert_eq!(
        rows[0].get::<_, &str>("admin_id").parse::<Uuid>().unwrap(),
        database.admin_id(ADMIN_NAME).await
    );
    assert_eq!(
        rows[0].get::<_, f64>("sent_at"),
        server.clock.now().as_second() as f64
    );
    assert_eq!(rows[0].get::<_, f64>("valid_for"), 7.0 * 24.0 * 3600.0);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_link_works_once() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    let token = token_from_last_email(&server);

    let first = accept(&server, &token, NAME).await;
    let second = accept(&server, &token, "Ravi").await;
    let unknown = accept(
        &server,
        "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M",
        "Ravi",
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::NOT_FOUND);
    assert_eq!(second.1["type"], INVITE_NOT_FOUND);
    assert_eq!(second, unknown);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn accepting_one_link_twice_at_once_creates_one_user_and_the_other_gets_invite_not_found() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    for (email, names) in [
        (EMAIL, [NAME, NAME]),
        ("ravi@example.com", ["Ravi", "Ravi K"]),
    ] {
        invite(&server, &database, email).await.unwrap();
        let token = token_from_last_email(&server);

        let (first, second) = tokio::join!(
            accept(&server, &token, names[0]),
            accept(&server, &token, names[1])
        );

        let mut statuses = [first.0, second.0];
        statuses.sort();
        assert_eq!(statuses, [StatusCode::CREATED, StatusCode::NOT_FOUND]);
        let not_found = if first.0 == StatusCode::NOT_FOUND {
            first
        } else {
            second
        };
        assert_eq!(not_found.1["type"], INVITE_NOT_FOUND);
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_link_expires_7_days_after_the_invite_was_sent() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    let expiring = token_from_last_email(&server);
    invite(&server, &database, "ravi@example.com")
        .await
        .unwrap();
    let lasting = token_from_last_email(&server);

    server
        .clock
        .advance(SignedDuration::from_hours(7 * 24) - SignedDuration::from_secs(1));
    let before = accept(&server, &lasting, "Ravi").await;
    server.clock.advance(SignedDuration::from_secs(1));
    let after = accept(&server, &expiring, NAME).await;

    assert_eq!(before.0, StatusCode::CREATED);
    assert_eq!(after.0, StatusCode::NOT_FOUND);
    assert_eq!(after.1["type"], INVITE_NOT_FOUND);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_address_that_already_has_a_user_or_a_pending_invite_cannot_be_invited_in_any_capitals()
{
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, "Noor@Example.com")
        .await
        .unwrap();
    accept(&server, &token_from_last_email(&server), NAME).await;
    invite(&server, &database, "Ravi@Example.com")
        .await
        .unwrap();

    let user = invite(&server, &database, "NOOR@example.COM").await;
    let pending = invite(&server, &database, "ravi@EXAMPLE.com").await;

    for result in [user, pending] {
        let error = result.expect_err("the address is taken");
        assert!(error.find_source::<AddressTaken>().is_some(), "{error}");
    }
    assert_eq!(server.mailer.sent().len(), 2);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn inviting_one_address_twice_at_once_sends_one_invite() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (first, second) = tokio::join!(
        invite(&server, &database, EMAIL),
        invite(&server, &database, "NOOR@HOUSEHOLD.EXAMPLE")
    );

    let error = first.err().or(second.err()).expect("the address is taken");
    assert!(error.find_source::<AddressTaken>().is_some(), "{error}");
    assert_eq!(server.mailer.sent().len(), 1);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_taken_name_returns_409_and_leaves_the_invite_usable() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, "another password").await;
    invite(&server, &database, EMAIL).await.unwrap();
    let token = token_from_last_email(&server);

    let taken = accept(&server, &token, &NAME.to_uppercase()).await;
    let other = accept(&server, &token, "Ravi").await;

    assert_eq!(taken.0, StatusCode::CONFLICT);
    assert_eq!(taken.1["type"], "urn:selfless:problem:name-taken");
    assert_eq!(other.0, StatusCode::CREATED);
    assert_eq!(other.1["name"], "Ravi");
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_empty_password_returns_422_and_leaves_the_invite_usable() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    let token = token_from_last_email(&server);

    let empty = accept_with_password(&server, &token, NAME, "").await;
    let other = accept(&server, &token, NAME).await;

    assert_eq!(empty.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(empty.1["type"], "urn:selfless:problem:empty-password");
    assert_eq!(other.0, StatusCode::CREATED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn pending_invites_are_listed_newest_first_with_their_sent_time_and_expiry_including_expired_ones()
 {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let week = SignedDuration::from_hours(7 * 24);
    invite(&server, &database, EMAIL).await.unwrap();
    let expired_at = server.clock.now();
    server.clock.advance(SignedDuration::from_hours(8 * 24));
    invite(&server, &database, "ravi@example.com")
        .await
        .unwrap();
    let pending_at = server.clock.now();

    let invites = pending_invites(&server).await;

    assert_eq!(
        invites,
        [
            ("ravi@example.com".to_owned(), pending_at, pending_at + week),
            (EMAIL.to_owned(), expired_at, expired_at + week),
        ]
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_resent_invite_has_a_new_link_and_a_new_7_day_expiry_and_the_old_link_stops_working() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    let old = token_from_last_email(&server);
    server.clock.advance(SignedDuration::from_hours(24));
    let resent_at = server.clock.now();

    resend(&server, invite_id(&server, EMAIL).await).await;

    let sent = server.mailer.sent();
    let new = token_from_last_email(&server);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].recipient, EMAIL);
    assert_eq!(
        pending_invites(&server).await,
        [(
            EMAIL.to_owned(),
            resent_at,
            resent_at + SignedDuration::from_hours(7 * 24)
        )]
    );
    let with_old = accept(&server, &old, NAME).await;
    let with_new = accept(&server, &new, NAME).await;
    assert_eq!(with_old.0, StatusCode::NOT_FOUND);
    assert_eq!(with_old.1["type"], INVITE_NOT_FOUND);
    assert_eq!(with_new.0, StatusCode::CREATED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_cancelled_invite_is_no_longer_pending_and_its_link_stops_working() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();
    let token = token_from_last_email(&server);

    cancel(&server, invite_id(&server, EMAIL).await).await;

    let (status, problem) = accept(&server, &token, NAME).await;
    assert_eq!(pending_invites(&server).await, []);
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["type"], INVITE_NOT_FOUND);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn accepting_without_a_password_returns_a_bad_request_problem() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    invite(&server, &database, EMAIL).await.unwrap();

    let (status, problem) = server
        .request(
            Method::POST,
            "/api/invites/accept/v1",
            None,
            Some(json!({ "token": token_from_last_email(&server), "name": NAME })),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    drop(server);
    database.drop().await;
}
