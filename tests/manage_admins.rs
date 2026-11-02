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
