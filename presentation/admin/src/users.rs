use super::AdminAction;
use super::PAGE_TITLE_ID;
use super::admin_page;
use super::components::alert::AlertVariant;
use super::components::alert::alert;
use super::components::alert::alert_title;
use super::components::alert_dialog::alert_dialog;
use super::components::badge::BadgeVariant;
use super::components::badge::badge;
use super::components::button::ButtonSize;
use super::components::button::ButtonVariant;
use super::components::button::button;
use super::components::dialog::dialog;
use super::components::dialog::dialog_content;
use super::components::dialog::dialog_description;
use super::components::dialog::dialog_footer;
use super::components::dialog::dialog_header;
use super::components::dialog::dialog_title;
use super::components::field::field;
use super::components::field::field_error;
use super::components::field::field_label;
use super::components::input::input;
use super::components::table::table;
use super::components::table::table_body;
use super::components::table::table_cell;
use super::components::table::table_head;
use super::components::table::table_header;
use super::components::table::table_row;
use super::csrf_field;
use super::local_time::TimeFormat;
use super::local_time::local_time;
use super::log_admin_action;
use super::require_admin;
use super::verify_csrf;
use application::EmptyPassword;
use application::create_invite::AddressTaken;
use application::create_user::NameTaken;
use application::rename_user::EmptyName;
use domain::Colour;
use domain::Symbol;
use domain::User;
use infrastructure::AppState;
use ohno::ErrorExt;
use serde::Deserialize;
use std::sync::LazyLock;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::icon::IconData;
use topcoat::icon::icon;
use topcoat::router::HeaderName;
use topcoat::router::HeaderValue;
use topcoat::router::RouterBuilder;
use topcoat::router::content::Form;
use topcoat::router::error::bad_request;
use topcoat::router::error::not_found;
use topcoat::router::header::ORIGIN;
use topcoat::router::page;
use topcoat::router::path_param;
use topcoat::router::request::headers;
use topcoat::router::response::response_headers;
use topcoat::router::route;
use topcoat::view::View;
use topcoat::view::ViewExt;
use topcoat::view::ViewHandle;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::svg::ViewBox;
use topcoat::view::view;
use uuid::Uuid;

pub(crate) const USERS_PATH: &str = "/admin/users";
const USERS_ID: &str = "users";
const USER_ID: &str = "user";
const INVITE_DIALOG_ID: &str = "invite-dialog";
const INVITE_FORM_ID: &str = "invite-form";
const RENAME_DIALOG_ID: &str = "rename-user-dialog";
const RENAME_FORM_ID: &str = "rename-user-form";
const RESET_PASSWORD_DIALOG_ID: &str = "reset-password-dialog";
const RESET_PASSWORD_FORM_ID: &str = "reset-password-form";
const DELETE_DIALOG_ID: &str = "delete-user-dialog";
const DELETE_FORM_ID: &str = "delete-user-form";
const SWAP: &str = "outerHTML";

path_param!(invite_id: Uuid, error = not_found);
path_param!(user_id: Uuid, error = not_found);

static SYMBOLS: LazyLock<Symbols> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../ui/theme/symbols.json"))
        .expect("symbols.json holds every symbol")
});

#[derive(Deserialize)]
struct Symbols {
    symbol: SymbolMarkup,
}

#[derive(Deserialize)]
struct SymbolMarkup {
    ring: String,
    triangle: String,
    star: String,
    waves: String,
}

pub(crate) fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .page(users_page)
        .route(create_invite)
        .route(resend_invite)
        .route(cancel_invite)
        .page(user_page)
        .route(rename_user)
        .route(reset_password)
        .route(delete_user)
}

enum Notice {
    Done(String),
    Failed(&'static str),
}

#[page("/admin/users")]
async fn users_page(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    Ok(view! { admin_page(path: USERS_PATH, title: "Users", users_section(notice: None)) })
}

#[derive(Deserialize)]
struct InviteForm {
    #[serde(default)]
    csrf: String,
    email: String,
}

#[route(POST "/admin/users/invites")]
async fn create_invite(cx: &Cx, Form(form): Form<InviteForm>) -> topcoat::Result<ViewHandle> {
    let admin_id = require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let state = app_context::<AppState>(cx);
    let result =
        application::create_invite::call(state, admin_id, form.email.clone(), origin(cx)?).await;
    let invite_id = match result {
        Ok(invite_id) => invite_id,
        Err(error) if error.find_source::<AddressTaken>().is_some() => {
            retarget(cx, INVITE_FORM_ID);
            return view! { cx =>
                invite_form(
                    email: &form.email,
                    error: Some("A user or a pending invite already has this address."),
                )
            }
            .single()
            .await;
        }
        Err(error) => return Err(error.into()),
    };
    log_admin_action(AdminAction::InviteCreate, invite_id);
    let notice = Notice::Done(format!("Invite sent to {}.", form.email));
    view! { cx => users_section(notice: Some(notice)) }
        .single()
        .await
}

#[derive(Deserialize)]
struct ActionForm {
    #[serde(default)]
    csrf: String,
}

#[route(POST "/admin/users/invites/{invite_id}/resend")]
async fn resend_invite(cx: &Cx, Form(form): Form<ActionForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let invite_id = *path_param::<InviteId>(cx)?;
    let state = app_context::<AppState>(cx);
    let notice = match pending_invite_email(state, invite_id).await? {
        Some(email) => {
            application::resend_invite::call(state, invite_id, origin(cx)?).await?;
            log_admin_action(AdminAction::InviteResend, invite_id);
            Notice::Done(format!("Invite sent again to {email}."))
        }
        None => Notice::Failed("This invite no longer exists."),
    };
    view! { cx => users_section(notice: Some(notice)) }
        .single()
        .await
}

#[route(POST "/admin/users/invites/{invite_id}/cancel")]
async fn cancel_invite(cx: &Cx, Form(form): Form<ActionForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let invite_id = *path_param::<InviteId>(cx)?;
    let state = app_context::<AppState>(cx);
    let notice = match pending_invite_email(state, invite_id).await? {
        Some(email) => {
            application::cancel_invite::call(state, invite_id).await?;
            log_admin_action(AdminAction::InviteCancel, invite_id);
            Notice::Done(format!("Cancelled the invite to {email}."))
        }
        None => Notice::Failed("This invite no longer exists."),
    };
    view! { cx => users_section(notice: Some(notice)) }
        .single()
        .await
}

#[page("/admin/users/{user_id}")]
async fn user_page(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    let user = find_user(cx).await?;
    Ok(view! {
        admin_page(path: USERS_PATH, title: &user.name, user_section(user: &user, notice: None))
    })
}

#[derive(Deserialize)]
struct RenameForm {
    #[serde(default)]
    csrf: String,
    name: String,
}

#[route(POST "/admin/users/{user_id}/name")]
async fn rename_user(cx: &Cx, Form(form): Form<RenameForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let mut user = find_user(cx).await?;
    let result =
        application::rename_user::call(app_context::<AppState>(cx), user.id, form.name.clone())
            .await;
    let error = match result {
        Ok(()) => None,
        Err(error) if error.find_source::<NameTaken>().is_some() => {
            Some("A user already has this name.")
        }
        Err(error) if error.find_source::<EmptyName>().is_some() => Some("Enter a name."),
        Err(error) => return Err(error.into()),
    };
    if let Some(error) = error {
        retarget(cx, RENAME_FORM_ID);
        return view! { cx => rename_form(user: &user, typed: &form.name, error: Some(error)) }
            .single()
            .await;
    }
    log_admin_action(AdminAction::UserRename, user.id);
    let notice = Notice::Done(format!("Renamed {} to {}.", user.name, form.name));
    user.name = form.name;
    view! { cx =>
        user_section(user: &user, notice: Some(notice))
        <span hx-swap-oob=(format!("innerHTML:#{PAGE_TITLE_ID}"))>(&user.name)</span>
    }
    .single()
    .await
}

#[derive(Deserialize)]
struct ResetPasswordForm {
    #[serde(default)]
    csrf: String,
    password: String,
}

#[route(POST "/admin/users/{user_id}/password")]
async fn reset_password(
    cx: &Cx,
    Form(form): Form<ResetPasswordForm>,
) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let user = find_user(cx).await?;
    let result =
        application::reset_user_password::call(app_context::<AppState>(cx), user.id, form.password)
            .await;
    match result {
        Ok(()) => {}
        Err(error) if error.find_source::<EmptyPassword>().is_some() => {
            retarget(cx, RESET_PASSWORD_FORM_ID);
            return view! { cx =>
                reset_password_form(user: &user, error: Some("Enter a new password."))
            }
            .single()
            .await;
        }
        Err(error) => return Err(error.into()),
    }
    log_admin_action(AdminAction::UserResetPassword, user.id);
    let notice = Notice::Done(format!(
        "Reset {}'s password and signed them out everywhere.",
        user.name
    ));
    view! { cx => user_section(user: &user, notice: Some(notice)) }
        .single()
        .await
}

#[derive(Deserialize)]
struct DeleteForm {
    #[serde(default)]
    csrf: String,
    name: String,
}

#[route(POST "/admin/users/{user_id}/delete")]
async fn delete_user(cx: &Cx, Form(form): Form<DeleteForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let user = find_user(cx).await?;
    if form.name.to_lowercase() != user.name.to_lowercase() {
        retarget(cx, DELETE_FORM_ID);
        return view! { cx =>
            delete_form(user: &user, typed: &form.name, error: Some("The name does not match."))
        }
        .single()
        .await;
    }
    application::delete_user::call(app_context::<AppState>(cx), user.id).await?;
    log_admin_action(AdminAction::UserDelete, user.id);
    view! { cx =>
        <div id=(USER_ID) class="flex flex-col gap-6">
            notice_alert(notice: Notice::Done(format!("Deleted {} and their watch history.", user.name)))
            <a href=(USERS_PATH) class="text-sm underline underline-offset-4">"Back to Users"</a>
        </div>
    }
    .single()
    .await
}

async fn find_user(cx: &Cx) -> topcoat::Result<User> {
    let user_id = *path_param::<UserId>(cx)?;
    application::get_user::call(app_context::<AppState>(cx), user_id)
        .await?
        .ok_or_else(|| not_found().into())
}

async fn pending_invite_email(
    state: &AppState,
    invite_id: Uuid,
) -> topcoat::Result<Option<String>> {
    Ok(application::list_pending_invites::call(state)
        .await?
        .into_iter()
        .find(|x| x.id == invite_id)
        .map(|x| x.email))
}

fn origin(cx: &Cx) -> topcoat::Result<String> {
    headers(cx)
        .get(ORIGIN)
        .and_then(|x| x.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| bad_request("the request has no Origin header").into())
}

fn retarget(cx: &Cx, id: &str) {
    let target = HeaderValue::from_str(&format!("#{id}")).expect("element ids are header-safe");
    response_headers(cx).append(HeaderName::from_static("hx-retarget"), target);
    response_headers(cx).append(
        HeaderName::from_static("hx-reswap"),
        HeaderValue::from_static(SWAP),
    );
}

fn case_insensitive_pattern(name: &str) -> String {
    name.chars()
        .map(|x| {
            let lower = x.to_lowercase().collect::<String>();
            let upper = x.to_uppercase().collect::<String>();
            if lower != upper && lower.chars().count() == 1 && upper.chars().count() == 1 {
                format!("[{lower}{upper}]")
            } else if r"^$\.*+?()[]{}|/".contains(x) {
                format!(r"\{x}")
            } else {
                x.to_string()
            }
        })
        .collect()
}

fn symbol_icon(symbol: Symbol) -> IconData {
    let symbols = &SYMBOLS.symbol;
    let markup = match symbol {
        Symbol::Ring => &symbols.ring,
        Symbol::Triangle => &symbols.triangle,
        Symbol::Star => &symbols.star,
        Symbol::Waves => &symbols.waves,
    };
    IconData::unescaped_unchecked(ViewBox::new(0.0, 0.0, 200.0, 200.0), markup)
}

fn colour_class(colour: Colour) -> &'static str {
    match colour {
        Colour::Amber => "bg-user-amber",
        Colour::Blue => "bg-user-blue",
        Colour::Pink => "bg-user-pink",
        Colour::Emerald => "bg-user-emerald",
    }
}

#[component]
async fn notice_alert(notice: Notice) -> topcoat::Result<impl View> {
    let (variant, message) = match notice {
        Notice::Done(message) => (AlertVariant::Neutral, message),
        Notice::Failed(message) => (AlertVariant::Destructive, message.to_owned()),
    };
    Ok(view! {
        alert(variant: variant, attrs: attributes! { role="status" }, alert_title((message)))
    })
}

#[component]
async fn users_section(cx: &Cx, notice: Option<Notice>) -> topcoat::Result<impl View> {
    let state = app_context::<AppState>(cx);
    let users = application::list_users::call(state).await?;
    let invites = application::list_pending_invites::call(state).await?;
    let now = state.clock.call();
    Ok(view! {
        <div id=(USERS_ID) class="flex flex-col gap-6">
            if let Some(notice) = notice {
                notice_alert(notice: notice)
            }
            <div class="flex justify-end">
                button(
                    attrs: attributes! {
                        type="button"
                        commandfor=(INVITE_DIALOG_ID)
                        command="show-modal"
                    },
                    "Invite user"
                )
            </div>
            table(
                table_header(
                    table_row(
                        table_head(<span class="sr-only">"Colour and symbol"</span>)
                        table_head("Name")
                        table_head("Created")
                        table_head("Signed in on")
                        table_head("Last used")
                    )
                )
                table_body(
                    for listed in &users {
                        table_row(
                            attrs: attributes! { class="relative" },
                            table_cell(
                                <span class=(format!(
                                    "inline-flex size-8 items-center justify-center rounded-full text-2xl {}",
                                    colour_class(listed.user.colour),
                                ))>
                                    icon(data: symbol_icon(listed.user.symbol))
                                </span>
                            )
                            table_cell(
                                <a
                                    href=(format!("{USERS_PATH}/{}", listed.user.id))
                                    class="font-medium after:absolute after:inset-0"
                                >
                                    (&listed.user.name)
                                </a>
                            )
                            table_cell(local_time(at: listed.user.created_at, format: TimeFormat::Absolute))
                            table_cell((listed.signed_in_devices.to_string()))
                            table_cell(
                                if let Some(last_used_at) = listed.last_used_at {
                                    local_time(
                                        at: last_used_at,
                                        format: TimeFormat::Relative,
                                        hour_precision: true,
                                    )
                                } else {
                                    "Never"
                                }
                            )
                        )
                    }
                )
            )
            <h2 class="text-base font-semibold">"Pending invites"</h2>
            if invites.is_empty() {
                <p class="text-sm text-muted-foreground">"No pending invites."</p>
            } else {
                table(
                    table_header(
                        table_row(
                            table_head("Email address")
                            table_head("Sent")
                            table_head("Expires")
                            table_head(<span class="sr-only">"Actions"</span>)
                        )
                    )
                    table_body(
                        for invite in &invites {
                            table_row(
                                table_cell((&invite.email))
                                table_cell(local_time(at: invite.sent_at, format: TimeFormat::Absolute))
                                table_cell(local_time(at: invite.expires_at, format: TimeFormat::Absolute))
                                table_cell(
                                    <div class="flex items-center justify-end gap-2">
                                        if invite.expired(now) {
                                            badge(variant: BadgeVariant::Destructive, "Expired")
                                        }
                                        <form
                                            method="post"
                                            action=(format!("{USERS_PATH}/invites/{}/resend", invite.id))
                                            hx-post=(format!("{USERS_PATH}/invites/{}/resend", invite.id))
                                            hx-target=(format!("#{USERS_ID}"))
                                            hx-swap=(SWAP)
                                        >
                                            csrf_field()
                                            button(
                                                variant: ButtonVariant::Outline,
                                                size: ButtonSize::Sm,
                                                attrs: attributes! { type="submit" },
                                                "Resend"
                                            )
                                        </form>
                                        button(
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Sm,
                                            attrs: attributes! {
                                                type="button"
                                                commandfor=(format!("cancel-invite-{}", invite.id))
                                                command="show-modal"
                                            },
                                            "Cancel"
                                        )
                                    </div>
                                )
                            )
                        }
                    )
                )
            }
            for invite in &invites {
                alert_dialog(
                    open: false,
                    attrs: attributes! { id=(format!("cancel-invite-{}", invite.id)) },
                    dialog_content(
                        dialog_header(
                            dialog_title("Cancel this invite?")
                            dialog_description(
                                (format!("The link sent to {} stops working.", invite.email))
                            )
                        )
                        <form
                            method="post"
                            action=(format!("{USERS_PATH}/invites/{}/cancel", invite.id))
                            hx-post=(format!("{USERS_PATH}/invites/{}/cancel", invite.id))
                            hx-target=(format!("#{USERS_ID}"))
                            hx-swap=(SWAP)
                        >
                            csrf_field()
                            dialog_footer(
                                button(
                                    variant: ButtonVariant::Outline,
                                    attrs: attributes! {
                                        type="button"
                                        commandfor=(format!("cancel-invite-{}", invite.id))
                                        command="close"
                                    },
                                    "Keep invite"
                                )
                                button(
                                    variant: ButtonVariant::Destructive,
                                    attrs: attributes! { type="submit" },
                                    "Cancel invite"
                                )
                            )
                        </form>
                    )
                )
            }
            dialog(
                open: false,
                attrs: attributes! { id=(INVITE_DIALOG_ID) },
                dialog_content(
                    dialog_header(
                        dialog_title("Invite user")
                        dialog_description(
                            "selfless emails them a link to choose their name and password. The link works once and expires in 7 days."
                        )
                    )
                    invite_form(email: "", error: None)
                )
            )
        </div>
    })
}

#[component]
async fn invite_form(email: &str, error: Option<&'static str>) -> topcoat::Result<impl View> {
    Ok(view! {
        <form
            id=(INVITE_FORM_ID)
            method="post"
            action="/admin/users/invites"
            hx-post="/admin/users/invites"
            hx-target=(format!("#{USERS_ID}"))
            hx-swap=(SWAP)
            class="flex flex-col gap-4"
        >
            csrf_field()
            field(
                field_label(attrs: attributes! { for="invite-email" }, "Email address")
                input(attrs: attributes! {
                    id="invite-email"
                    type="email"
                    name="email"
                    value=(email)
                    required=(true)
                    aria-invalid=(error.map(|_| "true"))
                    aria-describedby=(error.map(|_| "invite-email-error"))
                })
                if let Some(error) = error {
                    field_error(attrs: attributes! { id="invite-email-error" }, (error))
                }
            )
            dialog_footer(
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(INVITE_DIALOG_ID)
                        command="close"
                    },
                    "Cancel"
                )
                button(attrs: attributes! { type="submit" }, "Send invite")
            )
        </form>
    })
}

#[component]
async fn user_section(user: &User, notice: Option<Notice>) -> topcoat::Result<impl View> {
    Ok(view! {
        <div id=(USER_ID) class="flex flex-col gap-6">
            if let Some(notice) = notice {
                notice_alert(notice: notice)
            }
            <section class="flex flex-col items-start gap-2">
                <p class="text-sm text-muted-foreground">
                    "Fix a typo in their name. They stay signed in, and their apps and TVs show the new name."
                </p>
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(RENAME_DIALOG_ID)
                        command="show-modal"
                    },
                    "Rename"
                )
            </section>
            <section class="flex flex-col items-start gap-2">
                <p class="text-sm text-muted-foreground">
                    "Set a new password when they forget theirs. This signs them out everywhere."
                </p>
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(RESET_PASSWORD_DIALOG_ID)
                        command="show-modal"
                    },
                    "Reset password"
                )
            </section>
            <section class="flex flex-col items-start gap-2">
                <p class="text-sm text-muted-foreground">
                    "Deleting a user also deletes their watch history."
                </p>
                button(
                    variant: ButtonVariant::Destructive,
                    attrs: attributes! {
                        type="button"
                        commandfor=(DELETE_DIALOG_ID)
                        command="show-modal"
                    },
                    "Delete user"
                )
            </section>
            dialog(
                open: false,
                attrs: attributes! { id=(RENAME_DIALOG_ID) },
                dialog_content(
                    dialog_header(
                        dialog_title((format!("Rename {}", user.name)))
                        dialog_description("Names are unique, ignoring capitals.")
                    )
                    rename_form(user: user, typed: &user.name, error: None)
                )
            )
            dialog(
                open: false,
                attrs: attributes! { id=(RESET_PASSWORD_DIALOG_ID) },
                dialog_content(
                    dialog_header(
                        dialog_title((format!("Reset {}'s password", user.name)))
                        dialog_description("They are signed out everywhere and sign in again with the new password.")
                    )
                    reset_password_form(user: user, error: None)
                )
            )
            alert_dialog(
                open: false,
                attrs: attributes! { id=(DELETE_DIALOG_ID) },
                dialog_content(
                    dialog_header(
                        dialog_title((format!("Delete {}?", user.name)))
                        dialog_description(
                            "Their watch history goes too. Type their name to confirm."
                        )
                    )
                    delete_form(user: user, typed: "", error: None)
                )
            )
        </div>
    })
}

#[component]
async fn rename_form(
    user: &User,
    typed: &str,
    error: Option<&'static str>,
) -> topcoat::Result<impl View> {
    let action = format!("{USERS_PATH}/{}/name", user.id);
    Ok(view! {
        <form
            id=(RENAME_FORM_ID)
            method="post"
            action=(&action)
            hx-post=(&action)
            hx-target=(format!("#{USER_ID}"))
            hx-swap=(SWAP)
            class="flex flex-col gap-4"
        >
            csrf_field()
            field(
                field_label(attrs: attributes! { for="rename-name" }, "Name")
                input(attrs: attributes! {
                    id="rename-name"
                    type="text"
                    name="name"
                    value=(typed)
                    autocomplete="off"
                    required=(true)
                    aria-invalid=(error.map(|_| "true"))
                    aria-describedby=(error.map(|_| "rename-name-error"))
                })
                if let Some(error) = error {
                    field_error(attrs: attributes! { id="rename-name-error" }, (error))
                }
            )
            dialog_footer(
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(RENAME_DIALOG_ID)
                        command="close"
                    },
                    "Cancel"
                )
                button(attrs: attributes! { type="submit" }, "Save")
            )
        </form>
    })
}

#[component]
async fn reset_password_form(
    user: &User,
    error: Option<&'static str>,
) -> topcoat::Result<impl View> {
    let action = format!("{USERS_PATH}/{}/password", user.id);
    Ok(view! {
        <form
            id=(RESET_PASSWORD_FORM_ID)
            method="post"
            action=(&action)
            hx-post=(&action)
            hx-target=(format!("#{USER_ID}"))
            hx-swap=(SWAP)
            class="flex flex-col gap-4"
        >
            csrf_field()
            field(
                field_label(attrs: attributes! { for="new-password" }, "New password")
                input(attrs: attributes! {
                    id="new-password"
                    type="password"
                    name="password"
                    autocomplete="new-password"
                    required=(true)
                    aria-invalid=(error.map(|_| "true"))
                    aria-describedby=(error.map(|_| "new-password-error"))
                })
                if let Some(error) = error {
                    field_error(attrs: attributes! { id="new-password-error" }, (error))
                }
            )
            dialog_footer(
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(RESET_PASSWORD_DIALOG_ID)
                        command="close"
                    },
                    "Cancel"
                )
                button(attrs: attributes! { type="submit" }, "Reset")
            )
        </form>
    })
}

#[component]
async fn delete_form(
    user: &User,
    typed: &str,
    error: Option<&'static str>,
) -> topcoat::Result<impl View> {
    let action = format!("{USERS_PATH}/{}/delete", user.id);
    Ok(view! {
        <form
            id=(DELETE_FORM_ID)
            method="post"
            action=(&action)
            hx-post=(&action)
            hx-target=(format!("#{USER_ID}"))
            hx-swap=(SWAP)
            class="group flex flex-col gap-4"
        >
            csrf_field()
            field(
                field_label(attrs: attributes! { for="delete-name" }, "Name")
                input(attrs: attributes! {
                    id="delete-name"
                    type="text"
                    name="name"
                    value=(typed)
                    autocomplete="off"
                    required=(true)
                    pattern=(case_insensitive_pattern(&user.name))
                    placeholder=(&user.name)
                    aria-invalid=(error.map(|_| "true"))
                    aria-describedby=(error.map(|_| "delete-name-error"))
                })
                if let Some(error) = error {
                    field_error(attrs: attributes! { id="delete-name-error" }, (error))
                }
            )
            dialog_footer(
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="button"
                        commandfor=(DELETE_DIALOG_ID)
                        command="close"
                    },
                    "Cancel"
                )
                button(
                    variant: ButtonVariant::Destructive,
                    attrs: attributes! {
                        type="submit"
                        class="group-invalid:pointer-events-none group-invalid:opacity-50"
                    },
                    "Delete"
                )
            )
        </form>
    })
}
