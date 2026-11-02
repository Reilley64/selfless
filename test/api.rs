use crate::Availability;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ClientKind;
use domain::EmailAddress;
use domain::Password;
use std::path::PathBuf;
use test_app::TestApp;
use test_fixtures::TestServer;
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
