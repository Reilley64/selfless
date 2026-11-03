use test_app::TestApp;
use test_fixtures::TestServer;
use topcoat::router::Router;

pub(crate) async fn serve(app: &TestApp) -> TestServer {
    TestServer::serve(
        crate::register(Router::builder())
            .app_context(app.state.clone())
            .build(),
    )
    .await
}
