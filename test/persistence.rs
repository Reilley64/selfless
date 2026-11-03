use application::ports::persistence::DbContext;
use persistence::DatabaseSettings;
use test_fixtures::TestDatabase;

pub(crate) async fn db_context(database: &TestDatabase) -> DbContext {
    let settings = DatabaseSettings {
        url: database.url(),
    };
    let db = persistence::connect(&settings).await.unwrap();
    persistence::migrate(&db).await.unwrap();
    DbContext::new(db, settings.location())
}
