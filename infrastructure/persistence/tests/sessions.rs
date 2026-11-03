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
use domain::Session;
use persistence_test::db_context;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;
use uuid::Uuid;

const PASSWORD: &str = "a long passphrase";

async fn sign_in_on_tv(
    db_context: &DbContext,
    client_id: Uuid,
) -> Result<sign_in::Output, sign_in::Error> {
    sign_in::call(
        sign_in::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "Noor".to_owned(),
        PASSWORD.to_owned(),
        ClientKind::AppleTv,
        "Living Room".to_owned(),
        client_id,
    )
    .await
}

#[tokio::test]
async fn two_sign_ins_at_once_with_one_client_id_leave_one_session() {
    let database = TestDatabase::create().await;
    let mut db_context = db_context(&database).await;
    create_user::call(
        create_user::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "Noor".to_owned(),
        Password::new(PASSWORD.to_owned()).unwrap(),
        EmailAddress::new("noor@example.com".to_owned()).unwrap(),
    )
    .await
    .unwrap();

    for _ in 0..10 {
        let client_id = Uuid::new_v4();
        let (first, second) = tokio::join!(
            sign_in_on_tv(&db_context, client_id),
            sign_in_on_tv(&db_context, client_id)
        );

        first.unwrap();
        second.unwrap();
        let from_client = db_context
            .sessions()
            .filter(Session::fields().client_id().eq(client_id))
            .count()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        assert_eq!(from_client, 1);
    }
    drop(db_context);
    database.drop().await;
}
