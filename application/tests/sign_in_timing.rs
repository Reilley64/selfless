use application::admins::create_admin;
use application::ports::Clock;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::sessions::admin_sign_in;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ClientKind;
use domain::EmailAddress;
use domain::Password;
use std::time::Duration;
use std::time::Instant;
use test_fixtures::FakeClock;
use uuid::Uuid;

const PASSWORD: &str = "a long passphrase";

async fn time(sign_in: impl Future<Output = ()>) -> Duration {
    let start = Instant::now();
    sign_in.await;
    start.elapsed()
}

#[tokio::test]
async fn an_unknown_name_takes_as_long_as_a_wrong_password() {
    let db_context = DbContext::new(
        test_fixtures::mock_database(persistence::models()).await,
        DbLocation {
            database: "mock".to_owned(),
            address: "localhost".to_owned(),
            port: 0,
        },
    );
    let clock = FakeClock::new();
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
    create_admin::call(
        create_admin::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "Maya".to_owned(),
        Password::new(PASSWORD.to_owned()).unwrap(),
    )
    .await
    .unwrap();
    let sign_in = async |name: &str| {
        sign_in::call(
            sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            name.to_owned(),
            "wrong".to_owned(),
            ClientKind::Web,
            "Firefox on macOS".to_owned(),
            Uuid::new_v4(),
        )
        .await
        .unwrap();
    };
    let admin_sign_in = async |name: &str| {
        admin_sign_in::call(
            admin_sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            name.to_owned(),
            "wrong".to_owned(),
        )
        .await
        .unwrap();
    };

    let user_wrong_password = time(sign_in("Noor")).await;
    let user_unknown_name = time(sign_in("Theo")).await;
    let admin_wrong_password = time(admin_sign_in("Maya")).await;
    let admin_unknown_name = time(admin_sign_in("Theo")).await;

    assert!(
        user_unknown_name * 2 > user_wrong_password,
        "user: unknown name {user_unknown_name:?}, wrong password {user_wrong_password:?}"
    );
    assert!(
        admin_unknown_name * 2 > admin_wrong_password,
        "admin: unknown name {admin_unknown_name:?}, wrong password {admin_wrong_password:?}"
    );
}
