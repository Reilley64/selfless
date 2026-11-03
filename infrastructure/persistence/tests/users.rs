#[path = "../../../test/persistence.rs"]
mod persistence_test;

use application::ports::Clock;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ClientKind;
use domain::EmailAddress;
use domain::Password;
use domain::User;
use ohno::ErrorExt;
use persistence_test::db_context;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;
use uuid::Uuid;

async fn create_user(
    db_context: &DbContext,
    name: &str,
    email: &str,
) -> Result<create_user::Output, create_user::Error> {
    create_user::call(
        create_user::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        name.to_owned(),
        Password::new("a long passphrase".to_owned()).unwrap(),
        EmailAddress::new(email.to_owned()).unwrap(),
    )
    .await
}

#[tokio::test]
async fn user_names_are_unique_ignoring_case_and_shown_as_typed() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    create_user(&db_context, "Noor", "noor@example.com")
        .await
        .unwrap();

    let error = create_user(&db_context, "NOOR", "someone@example.com")
        .await
        .expect_err("the name is taken");
    let stored = db_context
        .users()
        .filter(User::fields().name().eq("noor"))
        .first()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .expect("the name matches ignoring case");

    assert!(
        error.find_source::<create_user::NameTaken>().is_some(),
        "{error}"
    );
    assert_eq!(stored.name, "Noor");
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn creating_two_users_with_one_name_at_once_creates_one() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;

    let (first, second) = tokio::join!(
        create_user(&db_context, "Noor", "noor@example.com"),
        create_user(&db_context, "NOOR", "someone@example.com")
    );

    let error = first.err().or(second.err()).expect("one name is taken");
    assert!(
        error.find_source::<create_user::NameTaken>().is_some(),
        "{error}"
    );
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn user_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    create_user(&db_context, "Noor", "noor@example.com")
        .await
        .unwrap();

    let signed_in = sign_in::call(
        sign_in::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "NOOR".to_owned(),
        "a long passphrase".to_owned(),
        ClientKind::Web,
        "Firefox".to_owned(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let sign_in::Output::SignedIn { name, .. } = signed_in else {
        panic!("the name matches ignoring case");
    };
    assert_eq!(name, "Noor");
    drop(db_context);
    database.drop().await;
}
