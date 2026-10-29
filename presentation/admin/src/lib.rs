pub mod components;
pub mod local_time;

use application::attributes::SELFLESS_ADMIN_ACTION;
use application::attributes::SELFLESS_ADMIN_ID;
use application::attributes::SELFLESS_ADMIN_TARGET_ID;
use components::button::ButtonSize;
use components::button::ButtonVariant;
use components::button::button;
use components::dropdown_menu::dropdown_menu;
use components::dropdown_menu::dropdown_menu_content;
use components::dropdown_menu::dropdown_menu_item;
use components::dropdown_menu::dropdown_menu_trigger;
use components::floating::FloatingAlign;
use components::floating::FloatingSide;
use components::icons::X;
use components::sidebar::SidebarMenuButtonSize;
use components::sidebar::SidebarMenuButtonVariant;
use components::sidebar::sidebar;
use components::sidebar::sidebar_content;
use components::sidebar::sidebar_footer;
use components::sidebar::sidebar_group;
use components::sidebar::sidebar_header;
use components::sidebar::sidebar_inset;
use components::sidebar::sidebar_menu;
use components::sidebar::sidebar_menu_button;
use components::sidebar::sidebar_menu_button_variants;
use components::sidebar::sidebar_menu_item;
use components::sidebar::sidebar_provider;
use components::sidebar::sidebar_trigger;
use infrastructure::AppState;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookie;
use topcoat::cookie::Cookies;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::cookie::SameSite;
use topcoat::cookie::cookies;
use topcoat::icon::icon;
use topcoat::router::RouterBuilder;
use topcoat::router::content::Css;
use topcoat::router::content::Form;
use topcoat::router::content::Js;
use topcoat::router::error::SeeOther;
use topcoat::router::error::forbidden;
use topcoat::router::error::see_other;
use topcoat::router::page;
use topcoat::router::request::headers;
use topcoat::router::route;
use topcoat::session::Token;
use topcoat::view::Child;
use topcoat::view::View;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::view;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

const HOME_PATH: &str = "/admin";
const SYNC_PATH: &str = "/admin/sync";
const SIGN_IN_PATH: &str = "/admin/sign-in";
const SIGN_OUT_PATH: &str = "/admin/sign-out";
const SESSION_COOKIE: &str = "session";
const CSRF_COOKIE: &str = "csrf";
const CSRF_HEADER: &str = "x-csrf-token";
const SIDEBAR_ID: &str = "sidebar";
const ACCOUNT_MENU_ID: &str = "account-menu";
const SECTIONS: [(&str, &str); 1] = [(SYNC_PATH, "Sync status")];
const HTMX_CONFIG: &str =
    r#"{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false}"#;

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .route(home)
        .page(sync)
        .page(sign_in_form)
        .page(sign_in)
        .route(sign_out)
        .route(styles)
        .route(htmx)
        .route(local_time_script)
        .cookies()
}

#[derive(Clone, Copy, Debug)]
pub enum AdminAction {
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
    SyncRetry,
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
            Self::SyncRetry => "sync.retry",
            Self::SyncConfirmRemoval => "sync.confirm_removal",
        }
    }
}

pub fn log_admin_action(action: AdminAction, target_id: Uuid) {
    tracing::info!(
        { SELFLESS_ADMIN_ACTION } = action.name(),
        { SELFLESS_ADMIN_TARGET_ID } = %target_id,
        "admin action"
    );
}

#[route(GET "/admin")]
async fn home() -> topcoat::Result<SeeOther> {
    Ok(see_other(SYNC_PATH))
}

#[page("/admin/sync")]
async fn sync(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    Ok(view! { admin_page(path: SYNC_PATH, title: "Sync status") })
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
    let token =
        application::admin_sign_in::call(app_context::<AppState>(cx), form.name, form.password)
            .await?;
    let Some(token) = token else {
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
        application::admin_sign_out::call(app_context::<AppState>(cx), session.value()).await?;
        cookies(cx).remove(admin_cookie(SESSION_COOKIE, String::new()));
    }
    Ok(see_other(SIGN_IN_PATH))
}

#[route(GET "/admin/assets/styles.css")]
async fn styles() -> topcoat::Result<Css<&'static str>> {
    Ok(Css(include_str!("../dist/styles.css")))
}

#[route(GET "/admin/assets/htmx.min.js")]
async fn htmx() -> topcoat::Result<Js<&'static str>> {
    Ok(Js(include_str!("../dist/htmx.min.js")))
}

#[route(GET "/admin/assets/time.js")]
async fn local_time_script() -> topcoat::Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/time.js")))
}

#[component]
async fn sign_in_page(failed: bool) -> topcoat::Result<impl View> {
    Ok(view! {
        document(
            <main class="flex min-h-svh items-center justify-center p-4">
                <form method="post" action="/admin/sign-in" class="flex w-full max-w-sm flex-col gap-4">
                    <h1 class="text-lg font-semibold">"selfless admin"</h1>
                    csrf_field()
                    if failed {
                        <p class="text-sm text-destructive">"Wrong name or password."</p>
                    }
                    <label class="flex flex-col gap-2 text-sm font-medium">
                        "Name"
                        <input
                            type="text"
                            name="name"
                            autocomplete="username"
                            required=(true)
                            class="h-9 rounded-lg border border-border bg-transparent px-3 font-normal outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        />
                    </label>
                    <label class="flex flex-col gap-2 text-sm font-medium">
                        "Password"
                        <input
                            type="password"
                            name="password"
                            autocomplete="current-password"
                            required=(true)
                            class="h-9 rounded-lg border border-border bg-transparent px-3 font-normal outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        />
                    </label>
                    button(attrs: attributes! { type="submit" }, "Sign in")
                </form>
            </main>
        )
    })
}

#[component]
async fn admin_page(
    cx: &Cx,
    path: &str,
    title: &str,
    #[default] child: Child<'_>,
) -> topcoat::Result<impl View> {
    let build_info = &app_context::<AppState>(cx).build_info;
    Ok(view! {
        document(
            sidebar_provider(
                sidebar(
                    sheet_attrs: attributes! { id=(SIDEBAR_ID) },
                    sidebar_header(
                        <div class="flex items-center justify-between">
                            <span class="px-2 text-sm font-semibold">"selfless admin"</span>
                            button(
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Icon,
                                attrs: attributes! {
                                    type="button"
                                    class="md:hidden"
                                    commandfor=(SIDEBAR_ID)
                                    command="close"
                                    aria-label="Close sidebar"
                                },
                                icon(data: X)
                            )
                        </div>
                    )
                    sidebar_content(
                        sidebar_group(
                            sidebar_menu(
                                for (section_path, label) in SECTIONS {
                                    sidebar_menu_item(
                                        sidebar_menu_button(
                                            href: Some(section_path),
                                            active: section_path == path,
                                            <span>(label)</span>
                                        )
                                    )
                                }
                            )
                        )
                    )
                    sidebar_footer(
                        dropdown_menu(
                            dropdown_menu_trigger(
                                target: ACCOUNT_MENU_ID,
                                attrs: attributes! {
                                    class=(sidebar_menu_button_variants(
                                        SidebarMenuButtonVariant::Default,
                                        SidebarMenuButtonSize::Md,
                                    ))
                                },
                                <span>"Account"</span>
                            )
                            dropdown_menu_content(
                                id: ACCOUNT_MENU_ID,
                                side: FloatingSide::Top,
                                align: FloatingAlign::Start,
                                <form method="post" action=(SIGN_OUT_PATH)>
                                    csrf_field()
                                    dropdown_menu_item(attrs: attributes! { type="submit" }, "Sign out")
                                </form>
                            )
                        )
                        <div class="px-2 text-xs text-muted-foreground">
                            <p>(build_info.name)</p>
                            <p>(build_info.version)</p>
                        </div>
                    )
                )
                sidebar_inset(
                    sidebar_header(
                        sidebar_trigger(
                            attrs: attributes! {
                                class="md:hidden"
                                commandfor=(SIDEBAR_ID)
                                command="show-modal"
                            }
                        )
                        <h1 class="text-base font-semibold">(title)</h1>
                    )
                    <div class="flex flex-col gap-6 p-6">(child)</div>
                )
            )
        )
    })
}

#[component]
async fn document(cx: &Cx, #[default] child: Child<'_>) -> topcoat::Result<impl View> {
    let htmx_headers = format!(r#"{{"X-CSRF-Token":"{}"}}"#, csrf_token(cx));
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="htmx-config" content=(HTMX_CONFIG) />
                <title>"selfless admin"</title>
                <link rel="stylesheet" href="/admin/assets/styles.css" />
                <script src="/admin/assets/htmx.min.js" defer=(true)></script>
                <script src="/admin/assets/time.js" defer=(true)></script>
            </head>
            <body class="bg-background font-sans text-foreground antialiased" hx-headers=(htmx_headers)>
                (child)
            </body>
        </html>
    })
}

#[component]
async fn csrf_field(cx: &Cx) -> topcoat::Result<impl View> {
    Ok(view! { <input type="hidden" name="csrf" value=(csrf_token(cx)) /> })
}

fn csrf_token(cx: &Cx) -> String {
    if let Some(cookie) = cookies(cx).get(CSRF_COOKIE) {
        return cookie.value().to_owned();
    }
    let token = Token::random().encode();
    cookies(cx).add(admin_cookie(CSRF_COOKIE, token.clone()));
    token
}

fn verify_csrf(cx: &Cx, form_token: &str) -> topcoat::Result<()> {
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

async fn require_admin(cx: &Cx) -> topcoat::Result<Uuid> {
    let admin_id = match cookies(cx).get(SESSION_COOKIE) {
        Some(session) => {
            application::get_current_admin::call(app_context::<AppState>(cx), session.value())
                .await?
        }
        None => None,
    };
    let Some(admin_id) = admin_id else {
        return Err(see_other(SIGN_IN_PATH).into());
    };
    Span::current().set_attribute(SELFLESS_ADMIN_ID, admin_id.to_string());
    Ok(admin_id)
}

fn admin_cookie(name: &'static str, value: String) -> Cookie<'static> {
    Cookie::build((name, value))
        .path(HOME_PATH)
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .build()
}
