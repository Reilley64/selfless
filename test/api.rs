use crate::Availability;
use application::ports::persistence::DbScope;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ArtworkKind;
use domain::ClientKind;
use domain::EmailAddress;
use domain::Episode;
use domain::Movie;
use domain::Password;
use domain::Season;
use domain::Show;
use std::path::PathBuf;
use test_app::TestApp;
use test_fixtures::TestServer;
use toasty::schema::Model;
use topcoat::router::Router;
use uuid::Uuid;

pub(crate) async fn serve(app: &TestApp) -> TestServer {
    serve_dir(app, std::env::temp_dir()).await
}

pub(crate) async fn serve_web_app(app: &TestApp) -> (TestServer, PathBuf) {
    let web_dir = std::env::temp_dir().join(format!("selfless-web-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&web_dir).unwrap();
    std::fs::write(web_dir.join("index.html"), "<title>selfless</title>").unwrap();
    std::fs::write(web_dir.join("app.js"), "export {};").unwrap();
    (serve_dir(app, web_dir.clone()).await, web_dir)
}

async fn serve_dir(app: &TestApp, web_dir: PathBuf) -> TestServer {
    let availability = Availability::default();
    availability.accept_traffic();
    TestServer::serve(crate::router(
        Router::builder(),
        app.state.clone(),
        availability,
        web_dir,
    ))
    .await
}

pub(crate) async fn signed_in_user(app: &TestApp, name: &str, password: &str) -> (Uuid, String) {
    create_user::call(
        &app.state,
        name.to_owned(),
        Password::new(password.to_owned()).unwrap(),
        EmailAddress::new(format!("{}@example.com", name.to_lowercase())).unwrap(),
    )
    .await
    .unwrap();
    let sign_in::Output::SignedIn { user_id, token, .. } = sign_in::call(
        &app.state,
        name.to_owned(),
        password.to_owned(),
        ClientKind::Web,
        "Firefox on macOS".to_owned(),
        Uuid::new_v4(),
    )
    .await
    .unwrap() else {
        panic!("wrong name or password");
    };
    (user_id, token)
}

pub(crate) async fn add_movie(app: &TestApp, title: &str) -> Uuid {
    let mut db_context = app.state.db_context.clone();
    movie(app, title)
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}

pub(crate) fn movie(app: &TestApp, title: &str) -> <Movie as Model>::Create {
    app.state
        .db_context
        .movies()
        .create()
        .tmdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
        .radarr_id(42)
        .title(title)
        .year(2016)
        .certification(Some("PG-13".to_owned()))
        .runtime_minutes(Some(116))
        .genres(vec!["Drama".to_owned(), "Science Fiction".to_owned()])
        .overview(Some(
            "A linguist works with the military to talk to aliens.".to_owned(),
        ))
        .directors(Vec::<String>::new())
        .visible(true)
}

pub(crate) async fn add_show(app: &TestApp, title: &str, season_numbers: &[i64]) -> Uuid {
    let mut db_context = app.state.db_context.clone();
    let tvdb_id = Uuid::new_v4().as_u64_pair().0 as i64;
    let show = Show::create()
        .tvdb_id(tvdb_id)
        .sonarr_id(tvdb_id)
        .title(title)
        .year(Some(2022))
        .certification(Some("TV-MA".to_owned()))
        .genres(vec!["Drama".to_owned(), "Mystery".to_owned()])
        .overview(Some("Office workers have their memories split.".to_owned()))
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    for number in season_numbers {
        let season = Season::create()
            .show_id(show.id)
            .number(*number)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        Episode::create()
            .season_id(season.id)
            .tvdb_id(Uuid::new_v4().as_u64_pair().0 as i64)
            .number(1)
            .title("Good News About Hell")
            .visible(true)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }
    show.id
}

pub(crate) async fn add_artwork(
    app: &TestApp,
    movie_id: Uuid,
    kind: ArtworkKind,
    path: &str,
) -> Uuid {
    let mut db_context = app.state.db_context.clone();
    db_context
        .artwork()
        .create()
        .movie_id(Some(movie_id))
        .kind(kind)
        .path(path)
        .modified_at(app.clock.now())
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}
