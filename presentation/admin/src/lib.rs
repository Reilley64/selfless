use application::attributes::SELFLESS_ADMIN_ID;
use application::sessions::admin_sign_in;
use application::sessions::admin_sign_out;
use application::sessions::get_current_admin;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookie;
use topcoat::cookie::Cookies;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::cookie::SameSite;
use topcoat::cookie::cookies;
use topcoat::router::RouterBuilder;
use topcoat::router::content::Form;
use topcoat::router::error::SeeOther;
use topcoat::router::error::forbidden;
use topcoat::router::error::see_other;
use topcoat::router::page;
use topcoat::router::route;
use topcoat::session::Token;
use topcoat::view::Child;
use topcoat::view::Unescaped;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const HOME_PATH: &str = "/admin";
const SIGN_IN_PATH: &str = "/admin/sign-in";
const SESSION_COOKIE: &str = "session";
const CSRF_COOKIE: &str = "csrf";

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .page(home)
        .page(sign_in_form)
        .page(sign_in)
        .route(sign_out)
        .cookies()
}

#[page("/admin")]
async fn home(cx: &Cx) -> topcoat::Result<impl View> {
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

#[page("/admin/sign-in")]
async fn sign_in_form() -> topcoat::Result<impl View> {
    Ok(view! { sign_in_page(failed: false) })
}

#[derive(Deserialize)]
struct SignIn {
    #[serde(default)]
    csrf: String,
    name: String,
    password: String,
}

#[page(POST "/admin/sign-in")]
async fn sign_in(cx: &Cx, Form(form): Form<SignIn>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let admin_sign_in::Output::SignedIn { token } =
        admin_sign_in::call(app_context::<AppState>(cx), form.name, form.password).await?
    else {
        return Ok(view! { sign_in_page(failed: true) });
    };
    cookies(cx).add(admin_cookie(SESSION_COOKIE, token));
    Err(see_other(HOME_PATH).into())
}

#[derive(Deserialize)]
struct SignOut {
    #[serde(default)]
    csrf: String,
}

#[route(POST "/admin/sign-out")]
async fn sign_out(cx: &Cx, Form(form): Form<SignOut>) -> topcoat::Result<SeeOther> {
    verify_csrf(cx, &form.csrf)?;
    if let Some(session) = cookies(cx).get(SESSION_COOKIE) {
        admin_sign_out::call(app_context::<AppState>(cx), session.value()).await?;
        cookies(cx).remove(admin_cookie(SESSION_COOKIE, String::new()));
    }
    Ok(see_other(SIGN_IN_PATH))
}

#[component]
async fn sign_in_page(failed: bool) -> topcoat::Result<impl View> {
    Ok(view! {
        document(
            <main class="flex flex-col gap-1">
                <form method="post" action="/admin/sign-in" class="flex flex-col gap-1">
                    csrf_field()
                    if failed {
                        <p class="font-sans text-text-primary">"Wrong name or password."</p>
                    }
                    <label class="font-sans text-text-primary">
                        "Name"
                        <input type="text" name="name" autocomplete="username" required=(true) />
                    </label>
                    <label class="font-sans text-text-primary">
                        "Password"
                        <input type="password" name="password" autocomplete="current-password" required=(true) />
                    </label>
                    <button type="submit" class="font-sans text-text-primary">"Sign in"</button>
                </form>
            </main>
        )
    })
}

#[component]
async fn document(#[default] child: Child<'_>) -> topcoat::Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"selfless admin"</title>
                <style>(Unescaped::new_unchecked(include_str!("../dist/styles.css")))</style>
            </head>
            <body class="bg-background p-web-inset">
                (child)
            </body>
        </html>
    })
}

#[component]
async fn csrf_field(cx: &Cx) -> topcoat::Result<impl View> {
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

fn verify_csrf(cx: &Cx, submitted: &str) -> topcoat::Result<()> {
    if cookies(cx)
        .get(CSRF_COOKIE)
        .is_some_and(|x| x.value() == submitted)
    {
        Ok(())
    } else {
        Err(forbidden().into())
    }
}

async fn require_admin(cx: &Cx) -> topcoat::Result<()> {
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

fn admin_cookie(name: &'static str, value: String) -> Cookie<'static> {
    Cookie::build((name, value))
        .path(HOME_PATH)
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .build()
}
