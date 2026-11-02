#[path = "../../../test/persistence.rs"]
mod persistence_test;

use application::admins::create_admin;
use application::admins::delete_admin;
use application::ports::AcquireLock;
use application::ports::Clock;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::sessions::admin_sign_in;
use domain::LastAdmin;
use domain::Password;
use ohno::ErrorExt;
use persistence_test::db_context;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple";

async fn create_admin(db_context: &mut DbContext, name: &str) -> Uuid {
    db_context
        .admins()
        .create()
        .name(name)
        .password_hash("hash")
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}

async fn create_admin_with_password(
    db_context: &DbContext,
    name: &str,
) -> Result<create_admin::Output, create_admin::Error> {
    create_admin::call(
        create_admin::Dependencies {
            db_context: db_context.clone(),
        },
        name.to_owned(),
        Password::new(PASSWORD.to_owned()).unwrap(),
    )
    .await
}

async fn admin_names(db_context: &mut DbContext) -> Vec<String> {
    db_context
        .admins()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.name)
        .collect()
}

async fn delete_admin(
    db_context: &DbContext,
    acquire_lock: &AcquireLock,
    id: Uuid,
) -> Result<delete_admin::Output, delete_admin::Error> {
    delete_admin::call(
        delete_admin::Dependencies {
            db_context: db_context.clone(),
            acquire_lock: acquire_lock.clone(),
        },
        id,
    )
    .await
}

#[tokio::test]
async fn two_admins_deleting_each_other_at_once_leave_one_admin() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    let maya = create_admin(&mut db_context, "Maya").await;
    let ravi = create_admin(&mut db_context, "Ravi").await;
    let acquire_lock = locking::acquire_lock();

    let (first, second) = tokio::join!(
        delete_admin(&db_context, &acquire_lock, maya),
        delete_admin(&db_context, &acquire_lock, ravi)
    );

    let failure = first.err().or(second.err()).expect("one delete fails");
    assert!(failure.find_source::<LastAdmin>().is_some(), "{failure}");
    let admins = db_context
        .admins()
        .all()
        .count()
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    assert_eq!(admins, 1);
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let db_context = db_context(&database).await;
    create_admin_with_password(&db_context, "Maya")
        .await
        .unwrap();

    let signed_in = admin_sign_in::call(
        admin_sign_in::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "MAYA".to_owned(),
        PASSWORD.to_owned(),
    )
    .await
    .unwrap();

    assert!(matches!(signed_in, admin_sign_in::Output::SignedIn { .. }));
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_are_unique_ignoring_case() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    create_admin_with_password(&db_context, "Maya")
        .await
        .unwrap();

    let error = create_admin_with_password(&db_context, "MAYA")
        .await
        .expect_err("the name is taken");

    assert!(
        error.find_source::<create_admin::NameTaken>().is_some(),
        "{error}"
    );
    assert_eq!(admin_names(&mut db_context).await, ["Maya"]);
    drop(db_context);
    database.drop().await;
}

#[tokio::test]
async fn creating_two_admins_with_one_name_at_once_creates_one() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;

    let (first, second) = tokio::join!(
        create_admin_with_password(&db_context, "Ravi"),
        create_admin_with_password(&db_context, "RAVI")
    );

    let error = first.err().or(second.err()).expect("one name is taken");
    assert!(
        error.find_source::<create_admin::NameTaken>().is_some(),
        "{error}"
    );
    assert_eq!(admin_names(&mut db_context).await.len(), 1);
    drop(db_context);
    database.drop().await;
}
