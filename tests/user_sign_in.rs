mod support;

use application::ports::persistence::DbScope;
use application::sessions::sign_in;
use domain::ClientKind;
use domain::Session;
use reqwest::StatusCode;
use serde_json::Value;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;
use uuid::Uuid;

async fn sign_in(server: &TestServer, name: &str, password: &str) -> (StatusCode, Value) {
    server.sign_in(name, password, "web").await
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
