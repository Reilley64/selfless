mod support;

use application::EmptyPassword;
use application::create_user::NameTaken;
use ohno::ErrorExt;
use reqwest::StatusCode;
use support::TestDatabase;
use support::TestServer;

#[tokio::test]
async fn new_users_get_the_colour_and_symbol_used_by_the_fewest_users() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let names = ["Noor", "Maya", "Ravi", "Ines", "Tomas"];
    for name in names {
        server.create_user(name, "password").await;
    }

    let mut looks = Vec::new();
    for name in names {
        let (status, body) = server.sign_in(name, "password", "web").await;
        assert_eq!(status, StatusCode::CREATED);
        looks.push((
            body["user"]["colour"].as_str().unwrap().to_owned(),
            body["user"]["symbol"].as_str().unwrap().to_owned(),
        ));
    }

    assert_eq!(
        looks,
        [
            ("amber", "ring"),
            ("blue", "triangle"),
            ("pink", "star"),
            ("emerald", "waves"),
            ("amber", "ring"),
        ]
        .map(|(colour, symbol)| (colour.to_owned(), symbol.to_owned()))
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn user_names_are_unique_ignoring_case_and_shown_as_typed() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user("Noor", "password").await;

    let error = server
        .try_create_user("NOOR", "another password")
        .await
        .expect_err("the name is taken");
    let (_, body) = server.sign_in("noor", "password", "web").await;

    assert!(error.find_source::<NameTaken>().is_some(), "{error}");
    assert_eq!(body["user"]["name"], "Noor");
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn creating_two_users_with_one_name_at_once_creates_one() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (first, second) = tokio::join!(
        server.try_create_user("Noor", "password"),
        server.try_create_user("NOOR", "another password")
    );

    let error = first.err().or(second.err()).expect("one name is taken");
    assert!(error.find_source::<NameTaken>().is_some(), "{error}");
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_user_password_cannot_be_empty() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let error = server
        .try_create_user("Noor", "")
        .await
        .expect_err("the password is empty");

    assert!(error.find_source::<EmptyPassword>().is_some(), "{error}");
    let (status, _) = server.sign_in("Noor", "", "web").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}
