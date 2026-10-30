use crate::HOME_PATH;
use crate::SESSION_COOKIE;
use crate::admin_cookie;
use crate::templates::sign_in::sign_in_template;
use crate::verify_csrf;
use application::sessions::admin_sign_in;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookies;
use topcoat::cookie::cookies;
use topcoat::router::content::Form;
use topcoat::router::error::see_other;
use topcoat::router::page;
use topcoat::view::View;
use topcoat::view::view;

#[page("/admin/sign-in")]
pub(crate) async fn sign_in_form() -> topcoat::Result<impl View> {
    Ok(view! { sign_in_template(failed: false) })
}

#[derive(Deserialize)]
pub(crate) struct SignIn {
    #[serde(default)]
    csrf: String,
    name: String,
    password: String,
}

#[page(POST "/admin/sign-in")]
pub(crate) async fn sign_in(cx: &Cx, Form(form): Form<SignIn>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let admin_sign_in::Output::SignedIn { token } =
        admin_sign_in::call(app_context::<AppState>(cx), form.name, form.password).await?
    else {
        return Ok(view! { sign_in_template(failed: true) });
    };
    cookies(cx).add(admin_cookie(cx, SESSION_COOKIE, token));
    Err(see_other(HOME_PATH).into())
}
