use crate::SESSION_COOKIE;
use crate::SIGN_IN_PATH;
use crate::admin_cookie;
use crate::verify_csrf;
use application::sessions::admin_sign_out;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookies;
use topcoat::cookie::cookies;
use topcoat::router::content::Form;
use topcoat::router::error::SeeOther;
use topcoat::router::error::see_other;
use topcoat::router::route;

#[derive(Deserialize)]
pub(crate) struct SignOut {
    #[serde(default)]
    csrf: String,
}

#[route(POST "/admin/sign-out")]
pub(crate) async fn sign_out(cx: &Cx, Form(form): Form<SignOut>) -> topcoat::Result<SeeOther> {
    verify_csrf(cx, &form.csrf)?;
    if let Some(session) = cookies(cx).get(SESSION_COOKIE) {
        admin_sign_out::call(app_context::<AppState>(cx), session.value()).await?;
        cookies(cx).remove(admin_cookie(cx, SESSION_COOKIE, String::new()));
    }
    Ok(see_other(SIGN_IN_PATH))
}

#[cfg(test)]
mod tests {
    use crate::test::serve;
    use application::admins::create_admin;
    use domain::Password;
    use http::StatusCode;
    use test_app::TestApp;

    #[tokio::test]
    async fn signing_out_ends_the_admin_session() {
        let app = TestApp::new().await;
        create_admin::call(
            &app.state,
            "Maya".to_owned(),
            Password::new("correct horse battery staple".to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let server = serve(&app).await;
        let mut browser = server.browser();
        browser
            .sign_in("Maya", "correct horse battery staple")
            .await;
        let token = browser.cookie("session").unwrap().to_owned();

        let signed_out = browser.sign_out().await;
        browser.set_cookie("session", &token);
        let home_with_old_token = browser.get("/admin").await;

        assert_eq!(signed_out.status, StatusCode::SEE_OTHER);
        assert_eq!(signed_out.location.as_deref(), Some("/admin/sign-in"));
        assert_eq!(home_with_old_token.status, StatusCode::SEE_OTHER);
        assert_eq!(
            home_with_old_token.location.as_deref(),
            Some("/admin/sign-in")
        );
    }
}
