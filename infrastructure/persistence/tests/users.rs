use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::users::create_user;
use domain::EmailAddress;
use domain::Password;
use domain::User;
use ohno::ErrorExt;
use persistence::DatabaseSettings;
use test_fixtures::TestDatabase;

async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}

async fn create_user(
    db_context: &DbContext,
    name: &str,
    email: &str,
) -> Result<create_user::Output, create_user::Error> {
    create_user::call(
        create_user::Dependencies {
            db_context: db_context.clone(),
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
