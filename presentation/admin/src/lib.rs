mod atoms;
pub mod components;
mod pages;
mod templates;

#[cfg(test)]
#[path = "../../../test/admin.rs"]
mod test;

use application::attributes::SELFLESS_ADMIN_ACTION;
use application::attributes::SELFLESS_ADMIN_ID;
use application::attributes::SELFLESS_ADMIN_TARGET_ID;
use application::sessions::get_current_admin;
use infrastructure::AppState;
use topcoat::asset::AssetBundle;
use topcoat::asset::RouterBuilderAssetExt;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookie;
use topcoat::cookie::Cookies;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::cookie::SameSite;
use topcoat::cookie::cookies;
use topcoat::router::Body;
use topcoat::router::HeaderValue;
use topcoat::router::LayerFn;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::RouterBuilder;
use topcoat::router::error::forbidden;
use topcoat::router::error::see_other;
use topcoat::router::header::CONTENT_SECURITY_POLICY;
use topcoat::router::request::headers;
use topcoat::router::response::response_headers;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

pub(crate) const HOME_PATH: &str = "/admin";
pub(crate) const SYNC_PATH: &str = "/admin/sync";
pub(crate) const USERS_PATH: &str = "/admin/users";
pub(crate) const ADMINS_PATH: &str = "/admin/admins";
pub(crate) const ACCOUNT_PASSWORD_PATH: &str = "/admin/account/password";
pub(crate) const SIGN_IN_PATH: &str = "/admin/sign-in";
pub(crate) const SIGN_OUT_PATH: &str = "/admin/sign-out";
pub(crate) const SESSION_COOKIE: &str = "session";
pub(crate) const CSRF_COOKIE: &str = "csrf";
const CSRF_HEADER: &str = "x-csrf-token";
const X_FORWARDED_PROTO: &str = "x-forwarded-proto";
const STRICT_CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-hashes' 'sha256-4AXVd4Ue0A6d/wfNkdhs1EwnaIBGk3lC7Q5dkpfGJEA='; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

pub fn register(router: RouterBuilder, assets: AssetBundle) -> RouterBuilder {
    pages::users::register(pages::sync::register(pages::admins::register(router)))
        .route(pages::home::home)
        .page(pages::sign_in::sign_in_form)
        .page(pages::sign_in::sign_in)
        .route(pages::sign_out::sign_out)
        .assets(assets)
        .cookies()
        .layer(LayerFn::new(
            Some(HOME_PATH),
            strict_content_security_policy,
        ))
}

fn strict_content_security_policy<'a>(cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
    response_headers(cx).append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(STRICT_CONTENT_SECURITY_POLICY),
    );
    next.run(cx, body)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AdminAction {
    InviteCreate,
    InviteResend,
    InviteCancel,
    UserRename,
    UserResetPassword,
    UserDelete,
    SessionEnd,
    SessionEndAll,
    AdminCreate,
    AdminSetPassword,
    AdminDelete,
    AccountChangePassword,
    SyncStart,
    SyncConfirmRemoval,
}

impl AdminAction {
    fn name(self) -> &'static str {
        match self {
            Self::InviteCreate => "invite.create",
            Self::InviteResend => "invite.resend",
            Self::InviteCancel => "invite.cancel",
            Self::UserRename => "user.rename",
            Self::UserResetPassword => "user.reset_password",
            Self::UserDelete => "user.delete",
            Self::SessionEnd => "session.end",
            Self::SessionEndAll => "session.end_all",
            Self::AdminCreate => "admin.create",
            Self::AdminSetPassword => "admin.set_password",
            Self::AdminDelete => "admin.delete",
            Self::AccountChangePassword => "account.change_password",
            Self::SyncStart => "sync.start",
            Self::SyncConfirmRemoval => "sync.confirm_removal",
        }
    }
}

pub(crate) fn log_admin_action(action: AdminAction, target_id: Uuid) {
    tracing::info!(
        { SELFLESS_ADMIN_ACTION } = action.name(),
        { SELFLESS_ADMIN_TARGET_ID } = %target_id,
        "admin action"
    );
}

pub(crate) fn verify_csrf(cx: &Cx, form_token: &str) -> topcoat::Result<()> {
    let submitted = headers(cx)
        .get(CSRF_HEADER)
        .and_then(|x| x.to_str().ok())
        .unwrap_or(form_token);
    if cookies(cx)
        .get(CSRF_COOKIE)
        .is_some_and(|x| x.value() == submitted)
    {
        Ok(())
    } else {
        Err(forbidden().into())
    }
}

pub(crate) async fn require_admin(cx: &Cx) -> topcoat::Result<Uuid> {
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
    Ok(admin_id)
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

#[cfg(test)]
mod tests {
    use crate::test::assets;
    use crate::test::serve;
    use application::admins::create_admin;
    use application::sessions::sign_in;
    use application::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use domain::Password;
    use http::StatusCode;
    use jiff::SignedDuration;
    use opentelemetry::Value;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use test_app::TestApp;
    use test_fixtures::Page;
    use test_fixtures::TestServer;
    use topcoat::context::Cx;
    use topcoat::router::Body;
    use topcoat::router::Layer;
    use topcoat::router::LayerFuture;
    use topcoat::router::Next;
    use topcoat::router::Path;
    use topcoat::router::Router;
    use topcoat::router::request::method;
    use topcoat::router::request::uri;
    use tracing::Instrument;
    use tracing_subscriber::layer::SubscriberExt;
    use uuid::Uuid;

    const NAME: &str = "Maya";
    const PASSWORD: &str = "correct horse battery staple";

    async fn create_admin(app: &TestApp) -> Uuid {
        create_admin::call(
            &app.state,
            NAME.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

    fn cookie_attributes(page: &Page, name: &str) -> Vec<String> {
        let cookie = page
            .set_cookies
            .iter()
            .find(|x| x.starts_with(&format!("{name}=")))
            .unwrap_or_else(|| panic!("no {name} cookie in {:?}", page.set_cookies));
        cookie.split("; ").skip(1).map(str::to_owned).collect()
    }

    #[tokio::test]
    async fn admin_pages_redirect_to_sign_in_without_an_admin_session() {
        let app = TestApp::new().await;
        let server = serve(&app).await;
        let mut browser = server.browser();

        let without_cookie = browser.get("/admin/sync").await;
        browser.set_cookie("session", "not-a-session");
        let with_unknown_cookie = browser.get("/admin/sync").await;

        for page in [without_cookie, with_unknown_cookie] {
            assert_eq!(page.status, StatusCode::SEE_OTHER);
            assert_eq!(page.location.as_deref(), Some("/admin/sign-in"));
        }
    }

    #[tokio::test]
    async fn the_admin_session_cookie_is_http_only_strict_scoped_to_admin_and_secure_only_over_https()
     {
        let app = TestApp::new().await;
        create_admin(&app).await;
        let server = serve(&app).await;
        let over_http = server.browser();
        let mut over_https = server.browser();
        over_https.set_header("x-forwarded-proto", "https");

        for (mut browser, secure) in [(over_http, false), (over_https, true)] {
            let form = browser.get("/admin/sign-in").await;
            let signed_in = browser.sign_in(NAME, PASSWORD).await;

            for attributes in [
                cookie_attributes(&form, "csrf"),
                cookie_attributes(&signed_in, "session"),
            ] {
                for attribute in ["HttpOnly", "SameSite=Strict", "Path=/admin"] {
                    assert!(attributes.contains(&attribute.to_owned()), "{attributes:?}");
                }
                assert_eq!(
                    attributes.contains(&"Secure".to_owned()),
                    secure,
                    "{attributes:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn admin_forms_without_the_csrf_token_are_rejected() {
        let app = TestApp::new().await;
        create_admin(&app).await;
        let server = serve(&app).await;
        let mut browser = server.browser();
        browser.get("/admin/sign-in").await;

        let sign_in_without_token = browser
            .post_form("/admin/sign-in", &[("name", NAME), ("password", PASSWORD)])
            .await;
        let sign_in_with_wrong_token = browser
            .post_form(
                "/admin/sign-in",
                &[("csrf", "wrong"), ("name", NAME), ("password", PASSWORD)],
            )
            .await;
        browser.sign_in(NAME, PASSWORD).await;
        let sign_out_without_token = browser.post_form("/admin/sign-out", &[]).await;
        let home = browser.get("/admin/sync").await;

        for page in [
            sign_in_without_token,
            sign_in_with_wrong_token,
            sign_out_without_token,
        ] {
            assert_eq!(page.status, StatusCode::FORBIDDEN);
        }
        assert_eq!(home.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn an_admin_session_ends_after_12_hours_without_use() {
        let app = TestApp::new().await;
        create_admin(&app).await;
        let server = serve(&app).await;
        let mut browser = server.browser();
        browser.sign_in(NAME, PASSWORD).await;

        app.clock.advance(SignedDuration::from_hours(12));
        let home = browser.get("/admin/sync").await;

        assert_eq!(home.status, StatusCode::SEE_OTHER);
        assert_eq!(home.location.as_deref(), Some("/admin/sign-in"));
    }

    #[tokio::test]
    async fn a_user_token_is_rejected_on_admin() {
        let app = TestApp::new().await;
        create_user::call(
            &app.state,
            "Noor".to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            EmailAddress::new("noor@example.com".to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let sign_in::Output::SignedIn { token, .. } = sign_in::call(
            &app.state,
            "Noor".to_owned(),
            "a long passphrase".to_owned(),
            ClientKind::Web,
            "Firefox on macOS".to_owned(),
            Uuid::new_v4(),
        )
        .await
        .unwrap() else {
            panic!("Noor signs in");
        };
        let server = serve(&app).await;
        let mut browser = server.browser();

        browser.set_cookie("session", &token);
        let home = browser.get("/admin/sync").await;

        assert_eq!(home.status, StatusCode::SEE_OTHER);
        assert_eq!(home.location.as_deref(), Some("/admin/sign-in"));
    }

    struct RequestSpanLayer;

    impl Layer for RequestSpanLayer {
        fn path(&self) -> Option<&Path> {
            None
        }

        fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
            let span = tracing::info_span!(
                "request",
                otel.name = format!("{} {}", method(cx), uri(cx).path())
            );
            Box::pin(next.run(cx, body).instrument(span))
        }
    }

    #[tokio::test]
    async fn admin_request_spans_carry_the_admin_id() {
        let app = TestApp::new().await;
        let admin_id = create_admin(&app).await;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );
        let server = TestServer::serve(
            crate::register(Router::builder(), assets())
                .layer(RequestSpanLayer)
                .app_context(app.state.clone())
                .build(),
        )
        .await;
        let mut browser = server.browser();
        browser.sign_in(NAME, PASSWORD).await;

        browser.get("/admin/sync").await;

        let spans = exporter.get_finished_spans().unwrap();
        let sync = spans
            .iter()
            .find(|x| x.name == "GET /admin/sync")
            .expect("a span for the sync status page");
        let attribute = sync
            .attributes
            .iter()
            .find(|x| x.key.as_str() == "selfless.admin.id")
            .expect("the selfless.admin.id attribute");
        assert_eq!(attribute.value, Value::from(admin_id.to_string()));
    }

    #[tokio::test]
    async fn htmx_requests_without_the_csrf_token_are_rejected() {
        let app = TestApp::new().await;
        create_admin(&app).await;
        let server = serve(&app).await;
        let mut browser = server.browser();
        browser.sign_in(NAME, PASSWORD).await;
        let sync = browser.get("/admin/sync").await;

        let without_token = browser.htmx_post("/admin/sign-out", None).await;
        let with_wrong_token = browser.htmx_post("/admin/sign-out", Some("wrong")).await;
        let still_signed_in = browser.get("/admin/sync").await;
        let with_token = browser
            .htmx_post("/admin/sign-out", Some(&sync.htmx_csrf_token()))
            .await;

        assert_eq!(without_token.status, StatusCode::FORBIDDEN);
        assert_eq!(with_wrong_token.status, StatusCode::FORBIDDEN);
        assert_eq!(still_signed_in.status, StatusCode::OK);
        assert_eq!(with_token.status, StatusCode::SEE_OTHER);
        assert_eq!(with_token.location.as_deref(), Some("/admin/sign-in"));
    }
}
