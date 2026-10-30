use crate::atoms::csrf_field::csrf_field;
use crate::require_admin;
use crate::templates::document::document;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::page;
use topcoat::view::View;
use topcoat::view::view;

#[page("/admin")]
pub(crate) async fn home(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    let build_info = &app_context::<AppState>(cx).build_info;
    Ok(view! {
        document(
            <main class="flex flex-col gap-1">
                <p class="font-sans font-bold text-text-primary">(build_info.name)</p>
                <p class="font-sans text-text-secondary">(build_info.version)</p>
                <form method="post" action="/admin/sign-out">
                    csrf_field()
                    <button type="submit" class="font-sans text-text-primary">"Sign out"</button>
                </form>
            </main>
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::test::serve;
    use domain::Password;
    use http::StatusCode;
    use test_app::TestApp;

    #[tokio::test]
    async fn the_first_admin_signs_in_and_sees_the_admin_app() {
        let app = TestApp::new().await;
        application::admins::create_admin::call(
            &app.state,
            "Maya".to_owned(),
            Password::new("correct horse battery staple".to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let server = serve(&app).await;
        let mut browser = server.browser();

        let signed_in = browser
            .sign_in("Maya", "correct horse battery staple")
            .await;
        let home = browser.get("/admin").await;

        assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
        assert_eq!(signed_in.location.as_deref(), Some("/admin"));
        assert_eq!(home.status, StatusCode::OK);
        let build_info = &app.state.build_info;
        for shown in [build_info.name, build_info.version] {
            assert!(home.body.contains(&format!(">{shown}<")), "{}", home.body);
        }
    }
}
