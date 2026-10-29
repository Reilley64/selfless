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
        cookies(cx).remove(admin_cookie(SESSION_COOKIE, String::new()));
    }
    Ok(see_other(SIGN_IN_PATH))
}
