mod support;

use application::EmptyPassword;
use application::create_admin::NameTaken;
use application::delete_admin::LastAdmin;
use ohno::ErrorExt;
use reqwest::StatusCode;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::Browser;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME;
use support::USER_PASSWORD;
use uuid::Uuid;

async fn create_admin(
    server: &TestServer,
    name: &str,
    password: &str,
) -> Result<Uuid, application::create_admin::Error> {
    application::create_admin::call(
        application::create_admin::Dependencies {
            db_context: server.db_context().await,
        },
        name.to_owned(),
        password.to_owned(),
    )
    .await
    .map(|x| x.id)
}

async fn delete_admin(
    server: &TestServer,
    id: Uuid,
) -> Result<(), application::delete_admin::Error> {
    application::delete_admin::call(
        application::delete_admin::Dependencies {
            db_context: server.db_context().await,
        },
        id,
    )
    .await
}

async fn signs_in(browser: &mut Browser, name: &str, password: &str) -> bool {
    browser.sign_in(name, password).await.status == StatusCode::SEE_OTHER
}

#[tokio::test]
async fn a_new_admin_signs_in_to_the_admin_app() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    create_admin(&server, "Ravi", "another password")
        .await
        .unwrap();

    let mut browser = server.browser();
    assert!(signs_in(&mut browser, "Ravi", "another password").await);
    assert_eq!(browser.get("/admin").await.status, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_are_unique_ignoring_case() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let error = create_admin(&server, &ADMIN_NAME.to_uppercase(), "another password")
        .await
        .expect_err("the name is taken");

    assert!(error.find_source::<NameTaken>().is_some(), "{error}");
    let mut browser = server.browser();
    assert!(signs_in(&mut browser, ADMIN_NAME, ADMIN_PASSWORD).await);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_and_user_names_are_separate_sets() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(USER_NAME, USER_PASSWORD).await;

    create_admin(&server, USER_NAME, "another password")
        .await
        .unwrap();
    server.create_user(ADMIN_NAME, "a third password").await;

    let mut browser = server.browser();
    assert!(signs_in(&mut browser, USER_NAME, "another password").await);
    let (status, _) = server.sign_in(ADMIN_NAME, "a third password", "web").await;
    assert_eq!(status, StatusCode::CREATED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_an_admin_ends_their_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = create_admin(&server, "Ravi", "another password")
        .await
        .unwrap();
    let mut ravi = server.browser();
    signs_in(&mut ravi, "Ravi", "another password").await;
    let mut maya = server.browser();
    signs_in(&mut maya, ADMIN_NAME, ADMIN_PASSWORD).await;

    delete_admin(&server, id).await.unwrap();

    assert_eq!(ravi.get("/admin").await.status, StatusCode::SEE_OTHER);
    assert!(!signs_in(&mut server.browser(), "Ravi", "another password").await);
    assert_eq!(maya.get("/admin").await.status, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_the_last_admin_fails() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let ravi = create_admin(&server, "Ravi", "another password")
        .await
        .unwrap();
    let maya = database.admin_id(ADMIN_NAME).await;
    delete_admin(&server, ravi).await.unwrap();
    let mut browser = server.browser();
    signs_in(&mut browser, ADMIN_NAME, ADMIN_PASSWORD).await;

    let error = delete_admin(&server, maya)
        .await
        .expect_err("the last admin stays");

    assert!(error.find_source::<LastAdmin>().is_some(), "{error}");
    assert_eq!(browser.get("/admin").await.status, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn changing_an_admins_password_ends_their_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    create_admin(&server, "Ravi", "another password")
        .await
        .unwrap();
    let maya = database.admin_id(ADMIN_NAME).await;
    let mut first = server.browser();
    signs_in(&mut first, ADMIN_NAME, ADMIN_PASSWORD).await;
    let mut second = server.browser();
    signs_in(&mut second, ADMIN_NAME, ADMIN_PASSWORD).await;
    let mut ravi = server.browser();
    signs_in(&mut ravi, "Ravi", "another password").await;

    application::change_admin_password::call(
        application::change_admin_password::Dependencies {
            db_context: server.db_context().await,
        },
        maya,
        "a new password".to_owned(),
    )
    .await
    .unwrap();

    assert_eq!(first.get("/admin").await.status, StatusCode::SEE_OTHER);
    assert_eq!(second.get("/admin").await.status, StatusCode::SEE_OTHER);
    assert_eq!(ravi.get("/admin").await.status, StatusCode::OK);
    let mut browser = server.browser();
    assert!(!signs_in(&mut browser, ADMIN_NAME, ADMIN_PASSWORD).await);
    assert!(signs_in(&mut browser, ADMIN_NAME, "a new password").await);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_passwords_cannot_be_empty() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let maya = database.admin_id(ADMIN_NAME).await;
    let mut browser = server.browser();
    signs_in(&mut browser, ADMIN_NAME, ADMIN_PASSWORD).await;

    let created = create_admin(&server, "Ravi", "")
        .await
        .expect_err("the password is empty");
    let changed = application::change_admin_password::call(
        application::change_admin_password::Dependencies {
            db_context: server.db_context().await,
        },
        maya,
        String::new(),
    )
    .await
    .expect_err("the password is empty");

    assert!(
        created.find_source::<EmptyPassword>().is_some(),
        "{created}"
    );
    assert!(
        changed.find_source::<EmptyPassword>().is_some(),
        "{changed}"
    );
    assert_eq!(browser.get("/admin").await.status, StatusCode::OK);
    assert!(!signs_in(&mut server.browser(), "Ravi", "").await);
    drop(server);
    database.drop().await;
}
