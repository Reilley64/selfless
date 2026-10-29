use crate::CSRF_COOKIE;
use crate::admin_cookie;
use topcoat::context::Cx;
use topcoat::cookie::Cookies;
use topcoat::cookie::cookies;
use topcoat::session::Token;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;

#[component]
pub(crate) async fn csrf_field(cx: &Cx) -> topcoat::Result<impl View> {
    let token = match cookies(cx).get(CSRF_COOKIE) {
        Some(cookie) => cookie.value().to_owned(),
        None => {
            let token = Token::random().encode();
            cookies(cx).add(admin_cookie(CSRF_COOKIE, token.clone()));
            token
        }
    };
    Ok(view! { <input type="hidden" name="csrf" value=(token) /> })
}
