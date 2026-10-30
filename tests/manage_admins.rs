mod support;

use application::admins::create_admin::NameTaken;
use domain::Password;
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
) -> Result<Uuid, application::admins::create_admin::Error> {
    application::admins::create_admin::call(
        application::admins::create_admin::Dependencies {
            db_context: server.db_context().await,
        },
        name.to_owned(),
        Password::new(password.to_owned()).unwrap(),
    )
    .await
    .map(|x| x.id)
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
async fn creating_two_admins_with_one_name_at_once_creates_one() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (first, second) = tokio::join!(
        create_admin(&server, "Ravi", "password"),
        create_admin(&server, "RAVI", "another password")
    );

    let error = first.err().or(second.err()).expect("one name is taken");
    assert!(error.find_source::<NameTaken>().is_some(), "{error}");
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

    application::admins::change_admin_password::call(
        application::admins::change_admin_password::Dependencies {
            db_context: server.db_context().await,
        },
        maya,
        Password::new("a new password".to_owned()).unwrap(),
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
