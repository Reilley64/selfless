use application::admins::delete_admin;
use application::ports::AcquireLock;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use domain::LastAdmin;
use ohno::ErrorExt;
use persistence::DatabaseSettings;
use test_fixtures::TestDatabase;
use uuid::Uuid;

async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}

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
