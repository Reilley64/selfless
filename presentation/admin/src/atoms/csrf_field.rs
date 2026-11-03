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
    Ok(view! { <input type="hidden" name="csrf" value=(csrf_token(cx)) /> })
}

pub(crate) fn csrf_token(cx: &Cx) -> String {
    if let Some(cookie) = cookies(cx).get(CSRF_COOKIE) {
        return cookie.value().to_owned();
    }
    let token = Token::random().encode();
    cookies(cx).add(admin_cookie(cx, CSRF_COOKIE, token.clone()));
    token
}
