use crate::SYNC_PATH;
use topcoat::router::error::SeeOther;
use topcoat::router::error::see_other;
use topcoat::router::route;

#[route(GET "/admin")]
pub(crate) async fn home() -> topcoat::Result<SeeOther> {
    Ok(see_other(SYNC_PATH))
}

#[cfg(test)]
mod tests {
    use crate::test::ADMIN_NAME;
    use crate::test::ADMIN_PASSWORD;
    use crate::test::create_admin;
    use crate::test::serve;
    use http::StatusCode;
    use test_app::TestApp;

    #[tokio::test]
    async fn the_first_admin_signs_in_and_lands_on_sync_status() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = server.browser();

        let signed_in = browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
        let home = browser.get("/admin").await;
        let sync = browser.get("/admin/sync").await;

        assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
        assert_eq!(signed_in.location.as_deref(), Some("/admin"));
        assert_eq!(home.status, StatusCode::SEE_OTHER);
        assert_eq!(home.location.as_deref(), Some("/admin/sync"));
        assert_eq!(sync.status, StatusCode::OK);
        assert!(sync.body.contains(">Sync status<"), "{}", sync.body);
    }
}
