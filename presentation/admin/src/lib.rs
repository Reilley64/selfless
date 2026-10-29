mod atoms;
mod pages;
mod templates;

use crate::pages::home::home;
use crate::pages::sign_in::sign_in;
use crate::pages::sign_in::sign_in_form;
use crate::pages::sign_out::sign_out;
use application::attributes::SELFLESS_ADMIN_ID;
use application::sessions::get_current_admin;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookie;
use topcoat::cookie::Cookies;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::cookie::SameSite;
use topcoat::cookie::cookies;
use topcoat::router::RouterBuilder;
use topcoat::router::error::forbidden;
use topcoat::router::error::see_other;
use topcoat::router::request::headers;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub(crate) const HOME_PATH: &str = "/admin";
pub(crate) const SIGN_IN_PATH: &str = "/admin/sign-in";
pub(crate) const SESSION_COOKIE: &str = "session";
pub(crate) const CSRF_COOKIE: &str = "csrf";
const X_FORWARDED_PROTO: &str = "x-forwarded-proto";

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .page(home)
        .page(sign_in_form)
        .page(sign_in)
        .route(sign_out)
        .cookies()
}

pub(crate) fn verify_csrf(cx: &Cx, submitted: &str) -> topcoat::Result<()> {
    if cookies(cx)
        .get(CSRF_COOKIE)
        .is_some_and(|x| x.value() == submitted)
    {
        Ok(())
    } else {
        Err(forbidden().into())
    }
}

pub(crate) async fn require_admin(cx: &Cx) -> topcoat::Result<()> {
    let current_admin = match cookies(cx).get(SESSION_COOKIE) {
        Some(session) => {
            get_current_admin::call(app_context::<AppState>(cx), session.value()).await?
        }
        None => get_current_admin::Output::NotSignedIn,
    };
    let get_current_admin::Output::SignedIn { admin_id } = current_admin else {
        return Err(see_other(SIGN_IN_PATH).into());
    };
    Span::current().set_attribute(SELFLESS_ADMIN_ID, admin_id.to_string());
    Ok(())
}

pub(crate) fn admin_cookie(cx: &Cx, name: &'static str, value: String) -> Cookie<'static> {
    Cookie::build((name, value))
        .path(HOME_PATH)
        .http_only(true)
        .secure(reached_traefik_over_https(cx))
        .same_site(SameSite::Strict)
        .build()
}

fn reached_traefik_over_https(cx: &Cx) -> bool {
    headers(cx)
        .get(X_FORWARDED_PROTO)
        .is_some_and(|x| x == "https")
}
