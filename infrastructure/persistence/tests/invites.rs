use application::invites::accept_invite;
use application::invites::create_invite;
use application::invites::send_invite_email;
use application::ports::AcquireLock;
use application::ports::Clock;
use application::ports::SendEmail;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use application::users::create_user;
use domain::EmailAddress;
use domain::Password;
use ohno::ErrorExt;
use persistence::DatabaseSettings;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use uuid::Uuid;

const ORIGIN: &str = "https://selfless.example";

async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}

struct Invites {
    db_context: DbContext,
    clock: FakeClock,
    mailer: FakeMailer,
    acquire_lock: AcquireLock,
}

impl Invites {
    async fn new(database: &TestDatabase) -> Self {
        Self {
            db_context: db_context(database).await,
            clock: FakeClock::new(),
            mailer: FakeMailer::default(),
            acquire_lock: locking::acquire_lock(),
        }
    }

    async fn invite(&self, email: &str) -> Result<create_invite::Output, create_invite::Error> {
        create_invite::call(
            create_invite::Dependencies {
                db_context: self.db_context.clone(),
                clock: Clock::new(self.clock.reader()),
            },
            Uuid::nil(),
            EmailAddress::new(email.to_owned()).unwrap(),
            ORIGIN.to_owned(),
        )
        .await
    }

    async fn send_invite(&self, email: &str) -> String {
        self.invite(email).await.unwrap();
        let mut db_context = self.db_context.clone();
        let invite_id = db_context
            .invites()
            .all()
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .id;
        send_invite_email::call(
            send_invite_email::Dependencies {
                db_context: self.db_context.clone(),
                clock: Clock::new(self.clock.reader()),
                acquire_lock: self.acquire_lock.clone(),
                send_email: SendEmail::new(self.mailer.sender()),
            },
            invite_id,
            ORIGIN.to_owned(),
        )
        .await
        .unwrap();
        let body = self.mailer.sent().last().unwrap().body.clone();
        body.split(&format!("{ORIGIN}/invite#"))
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    async fn accept(
        &self,
        token: &str,
        name: &str,
        idempotency_key: Uuid,
    ) -> Result<accept_invite::Output, accept_invite::Error> {
        accept_invite::call(
            accept_invite::Dependencies {
                db_context: self.db_context.clone(),
                clock: Clock::new(self.clock.reader()),
                acquire_lock: self.acquire_lock.clone(),
            },
            token,
            name.to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            idempotency_key,
        )
        .await
    }

    async fn counts(&self) -> (u64, u64, u64) {
        let mut db_context = self.db_context.clone();
        let invites = db_context
            .invites()
            .all()
            .count()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let outboxes = db_context
            .outboxes()
            .all()
            .count()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let users = db_context
            .users()
            .all()
            .count()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        (invites, outboxes, users)
    }
}

#[tokio::test]
async fn an_address_that_already_has_a_user_or_a_pending_invite_cannot_be_invited_in_any_capitals()
{
    let database = TestDatabase::create().await;
    let invites = Invites::new(&database).await;
    create_user::call(
        create_user::Dependencies {
            db_context: invites.db_context.clone(),
        },
        "Noor".to_owned(),
        Password::new("a long passphrase".to_owned()).unwrap(),
        EmailAddress::new("Noor@Example.com".to_owned()).unwrap(),
    )
    .await
    .unwrap();
    invites.invite("Ravi@Example.com").await.unwrap();

    let user = invites.invite("NOOR@example.COM").await;
    let pending = invites.invite("ravi@EXAMPLE.com").await;

    for result in [user, pending] {
        let error = result.expect_err("the address is taken");
        assert!(
            error.find_source::<create_invite::AddressTaken>().is_some(),
            "{error}"
        );
    }
    assert_eq!(invites.counts().await, (1, 1, 1));
    drop(invites);
    database.drop().await;
}

#[tokio::test]
async fn inviting_one_address_twice_at_once_queues_one_invite_and_one_email() {
    let database = TestDatabase::create().await;
    let invites = Invites::new(&database).await;

    let (first, second) = tokio::join!(
        invites.invite("noor@household.example"),
        invites.invite("NOOR@HOUSEHOLD.EXAMPLE")
    );

    let error = first.err().or(second.err()).expect("the address is taken");
    assert!(
        error.find_source::<create_invite::AddressTaken>().is_some(),
        "{error}"
    );
    assert_eq!(invites.counts().await, (1, 1, 0));
    drop(invites);
    database.drop().await;
}

#[tokio::test]
async fn accepting_one_link_twice_at_once_creates_one_user_and_the_other_gets_invite_not_found() {
    let database = TestDatabase::create().await;
    let invites = Invites::new(&database).await;

    for (email, names) in [
        ("noor@household.example", ["Noor", "Noor"]),
        ("ravi@household.example", ["Ravi", "Ravi K"]),
    ] {
        let token = invites.send_invite(email).await;

        let (first, second) = tokio::join!(
            invites.accept(&token, names[0], Uuid::new_v4()),
            invites.accept(&token, names[1], Uuid::new_v4())
        );

        let error = match (first, second) {
            (Ok(_), Err(error)) | (Err(error), Ok(_)) => error,
            other => panic!("expected one accept to succeed: {other:?}"),
        };
        assert!(
            error
                .find_source::<accept_invite::InviteNotFound>()
                .is_some(),
            "{error}"
        );
    }
    assert_eq!(invites.counts().await, (0, 2, 2));
    drop(invites);
    database.drop().await;
}

#[tokio::test]
async fn two_accepts_at_once_with_the_same_key_both_return_the_same_user() {
    let database = TestDatabase::create().await;
    let invites = Invites::new(&database).await;
    let token = invites.send_invite("noor@household.example").await;
    let key = Uuid::new_v4();

    let (first, second) = tokio::join!(
        invites.accept(&token, "Noor", key),
        invites.accept(&token, "Noor", key)
    );

    assert_eq!(first.unwrap().id, second.unwrap().id);
    assert_eq!(invites.counts().await.2, 1);
    drop(invites);
    database.drop().await;
}
