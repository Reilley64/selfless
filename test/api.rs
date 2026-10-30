use crate::Availability;
use test_app::TestApp;
use test_fixtures::TestServer;
use topcoat::router::Router;

pub(crate) async fn serve(app: &TestApp) -> TestServer {
    TestServer::serve(crate::router(
        Router::builder(),
        app.state.clone(),
        Availability::default(),
        std::env::temp_dir(),
    ))
    .await
}
