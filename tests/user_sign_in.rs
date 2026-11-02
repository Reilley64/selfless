mod support;

use application::ports::persistence::DbScope;
use application::sessions::sign_in;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::ClientKind;
use domain::Session;
use jiff::SignedDuration;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;
use tokio::time::Instant;
use uuid::Uuid;

async fn sign_in(server: &TestServer, name: &str, password: &str) -> (StatusCode, Value) {
    server.sign_in(name, password, "web").await
}

async fn me(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(Method::GET, "/api/me/v1", Some(token), None)
        .await
        .0
}

#[tokio::test]
async fn user_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let (status, body) = sign_in(&server, &NAME.to_uppercase(), PASSWORD).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user"]["name"], NAME);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_name_and_a_wrong_password_return_the_same_problem() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let wrong_name = sign_in(&server, "Maya", PASSWORD).await;
    let wrong_password = sign_in(&server, NAME, "wrong").await;

    assert_eq!(wrong_name.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_name.1["type"],
        "urn:selfless:problem:wrong-name-or-password"
    );
    assert_eq!(wrong_name, wrong_password);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_unknown_name_takes_as_long_as_a_wrong_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let sign_in_time = async |name: &str| {
        let start = Instant::now();
        sign_in(&server, name, "wrong").await;
        start.elapsed()
    };

    let wrong_password = sign_in_time(NAME).await;
    let unknown_name = sign_in_time("Maya").await;

    assert!(
        unknown_name * 2 > wrong_password,
        "unknown name {unknown_name:?}, wrong password {wrong_password:?}"
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_token_is_32_random_bytes_and_the_database_stores_only_its_sha_256_hash() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let (_, first) = sign_in(&server, NAME, PASSWORD).await;
    let (_, second) = sign_in(&server, NAME, PASSWORD).await;

    let tokens = [&first, &second].map(|x| x["token"].as_str().unwrap().to_owned());
    assert_ne!(tokens[0], tokens[1]);
    for token in &tokens {
        assert_eq!(URL_SAFE_NO_PAD.decode(token).unwrap().len(), 32);
    }
    let stored = database
        .query("SELECT token_hash FROM sessions")
        .await
        .iter()
        .map(|x| x.get::<_, Vec<u8>>("token_hash"))
        .collect::<Vec<_>>();
    assert_eq!(
        stored,
        tokens.map(|x| Sha256::digest(x.as_bytes()).to_vec())
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_out_ends_the_calling_session_only() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, first) = sign_in(&server, NAME, PASSWORD).await;
    let (_, second) = sign_in(&server, NAME, PASSWORD).await;
    let [first, second] = [&first, &second].map(|x| x["token"].as_str().unwrap().to_owned());

    let (status, _) = server
        .request(
            Method::DELETE,
            "/api/sessions/current/v1",
            Some(&first),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(me(&server, &first).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &second).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_web_session_ends_after_30_days_without_use() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, signed_in) = server.sign_in(NAME, PASSWORD, "web").await;
    let token = signed_in["token"].as_str().unwrap();

    server.clock.advance(SignedDuration::from_hours(29 * 24));
    let after_29_days = me(&server, token).await;
    server.clock.advance(SignedDuration::from_hours(29 * 24));
    let after_29_more_days = me(&server, token).await;
    server.clock.advance(SignedDuration::from_hours(30 * 24));
    let after_30_idle_days = me(&server, token).await;

    assert_eq!(after_29_days, StatusCode::OK);
    assert_eq!(after_29_more_days, StatusCode::OK);
    assert_eq!(after_30_idle_days, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn tv_sessions_never_expire_from_disuse() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, apple_tv) = server.sign_in(NAME, PASSWORD, "appleTv").await;
    let (_, google_tv) = server.sign_in(NAME, PASSWORD, "googleTv").await;

    server.clock.advance(SignedDuration::from_hours(400 * 24));

    for signed_in in [apple_tv, google_tv] {
        assert_eq!(
            me(&server, signed_in["token"].as_str().unwrap()).await,
            StatusCode::OK
        );
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn last_used_is_written_at_most_once_an_hour() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, signed_in) = sign_in(&server, NAME, PASSWORD).await;
    let token = signed_in["token"].as_str().unwrap();
    let last_used = async || {
        database
            .query("SELECT extract(epoch FROM last_used_at)::float8 AS last_used FROM sessions")
            .await[0]
            .get::<_, f64>("last_used")
    };
    let signed_in_at = last_used().await;

    server.clock.advance(SignedDuration::from_mins(59));
    me(&server, token).await;
    let after_59_minutes = last_used().await;
    server.clock.advance(SignedDuration::from_mins(1));
    me(&server, token).await;
    let after_an_hour = last_used().await;

    assert_eq!(after_59_minutes, signed_in_at);
    assert_eq!(after_an_hour - signed_in_at, 3600.0);
    drop(server);
    database.drop().await;
}

async fn sign_in_on_tv(
    server: &TestServer,
    name: &str,
    password: &str,
    client_id: Uuid,
    device_name: &str,
) -> sign_in::Output {
    sign_in::call(
        sign_in::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
        },
        name.to_owned(),
        password.to_owned(),
        ClientKind::AppleTv,
        device_name.to_owned(),
        client_id,
    )
    .await
    .unwrap()
}

fn token(output: sign_in::Output) -> String {
    match output {
        sign_in::Output::SignedIn { token, .. } => token,
        sign_in::Output::WrongNameOrPassword => panic!("wrong name or password"),
    }
}

async fn sessions(server: &TestServer) -> Vec<Session> {
    let mut db_context = server.db_context().await;
    db_context
        .sessions()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
}

#[tokio::test]
async fn signing_in_again_as_the_same_user_with_the_same_client_id_replaces_the_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let user_id = server.create_user(NAME, PASSWORD).await;
    let client_id = Uuid::new_v4();
    let old = token(sign_in_on_tv(&server, NAME, PASSWORD, client_id, "Living Room").await);
    let [first] = sessions(&server).await.try_into().unwrap();
    server.clock.advance(SignedDuration::from_hours(2));

    let new = token(sign_in_on_tv(&server, NAME, PASSWORD, client_id, "Bedroom").await);

    let [session] = sessions(&server).await.try_into().unwrap();
    assert_eq!(session.id, first.id);
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.client_id, client_id);
    assert_eq!(session.device_name, "Bedroom");
    assert_eq!(session.token_hash, Sha256::digest(new.as_bytes()).to_vec());
    assert_eq!(session.signed_in_at, server.clock.now());
    assert_eq!(session.last_used_at, server.clock.now());
    assert_eq!(me(&server, &old).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &new).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_in_with_a_different_client_id_keeps_both_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let living_room =
        token(sign_in_on_tv(&server, NAME, PASSWORD, Uuid::new_v4(), "Living Room").await);

    let bedroom = token(sign_in_on_tv(&server, NAME, PASSWORD, Uuid::new_v4(), "Bedroom").await);

    assert_eq!(sessions(&server).await.len(), 2);
    assert_eq!(me(&server, &living_room).await, StatusCode::OK);
    assert_eq!(me(&server, &bedroom).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn another_user_signing_in_with_the_same_client_id_keeps_both_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    server.create_user("Theo", "battery staple").await;
    let client_id = Uuid::new_v4();
    let noor = token(sign_in_on_tv(&server, NAME, PASSWORD, client_id, "Living Room").await);

    let theo =
        token(sign_in_on_tv(&server, "Theo", "battery staple", client_id, "Living Room").await);

    assert_eq!(sessions(&server).await.len(), 2);
    assert_eq!(me(&server, &noor).await, StatusCode::OK);
    assert_eq!(me(&server, &theo).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn two_sign_ins_at_once_with_one_client_id_leave_one_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let user_id = server.create_user(NAME, PASSWORD).await;

    for _ in 0..10 {
        let client_id = Uuid::new_v4();
        tokio::join!(
            sign_in_on_tv(&server, NAME, PASSWORD, client_id, "Living Room"),
            sign_in_on_tv(&server, NAME, PASSWORD, client_id, "Living Room"),
        );

        let from_client = sessions(&server)
            .await
            .into_iter()
            .filter(|x| x.user_id == user_id && x.client_id == client_id)
            .count();
        assert_eq!(from_client, 1);
    }
    drop(server);
    database.drop().await;
}
