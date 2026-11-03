use crate::AdminAction;
use crate::USERS_PATH;
use crate::atoms::csrf_field::csrf_field;
use crate::atoms::local_time::TimeFormat;
use crate::atoms::local_time::local_time;
use crate::components::alert::AlertVariant;
use crate::components::alert::alert;
use crate::components::alert::alert_title;
use crate::components::alert_dialog::alert_dialog;
use crate::components::badge::BadgeVariant;
use crate::components::badge::badge;
use crate::components::button::ButtonSize;
use crate::components::button::ButtonVariant;
use crate::components::button::button;
use crate::components::dialog::dialog;
use crate::components::dialog::dialog_content;
use crate::components::dialog::dialog_description;
use crate::components::dialog::dialog_footer;
use crate::components::dialog::dialog_header;
use crate::components::dialog::dialog_title;
use crate::components::field::field;
use crate::components::field::field_error;
use crate::components::field::field_label;
use crate::components::input::input;
use crate::components::table::table;
use crate::components::table::table_body;
use crate::components::table::table_cell;
use crate::components::table::table_head;
use crate::components::table::table_header;
use crate::components::table::table_row;
use crate::log_admin_action;
use crate::require_admin;
use crate::templates::admin_page::PAGE_TITLE_ID;
use crate::templates::admin_page::admin_page;
use crate::verify_csrf;
use application::invites::create_invite::AddressTaken;
use application::invites::list_pending_invites::EmailStatus;
use application::invites::resend_invite::InviteNotFound;
use application::sessions::list_user_sessions;
use application::users::get_user;
use application::users::rename_user::EmptyName;
use application::users::rename_user::NameTaken;
use domain::ClientKind;
use domain::Colour;
use domain::EmailAddress;
use domain::Password;
use infrastructure::AppState;
use ohno::ErrorExt;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
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
use topcoat::view::view;
use uuid::Uuid;

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
const SIGN_OUT_EVERYWHERE_DIALOG_ID: &str = "sign-out-everywhere-dialog";
const SWAP: &str = "outerHTML";

path_param!(invite_id: Uuid, error = not_found);
path_param!(user_id: Uuid, error = not_found);
path_param!(session_id: Uuid, error = not_found);

struct User {
    id: Uuid,
    name: String,
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
        .route(sign_out_device)
        .route(sign_out_everywhere)
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
    let error = match EmailAddress::new(form.email.clone()) {
        Ok(email) => {
            match application::invites::create_invite::call(state, admin_id, email, origin(cx)?)
                .await
            {
                Ok(invite) => {
                    log_admin_action(AdminAction::InviteCreate, invite.id);
                    None
                }
                Err(error) if error.find_source::<AddressTaken>().is_some() => {
                    Some("A user or a pending invite already has this address.")
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(_) => Some("Enter an email address."),
    };
    if let Some(error) = error {
        retarget(cx, INVITE_FORM_ID);
        return view! { cx => invite_form(email: &form.email, error: Some(error)) }
            .single()
            .await;
    }
    let notice = Notice::Done(format!("Sending an invite to {}.", form.email));
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
            match application::invites::resend_invite::call(state, invite_id, origin(cx)?).await {
                Ok(_) => {
                    log_admin_action(AdminAction::InviteResend, invite_id);
                    Notice::Done(format!("Sending a new invite to {email}."))
                }
                Err(error) if error.find_source::<InviteNotFound>().is_some() => {
                    Notice::Failed("This invite no longer exists.")
                }
                Err(error) => return Err(error.into()),
            }
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
            application::invites::cancel_invite::call(state, invite_id).await?;
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
    let result = application::users::rename_user::call(
        app_context::<AppState>(cx),
        user.id,
        form.name.clone(),
    )
    .await;
    let error = match result {
        Ok(_) => None,
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
    let Ok(password) = Password::new(form.password) else {
        retarget(cx, RESET_PASSWORD_FORM_ID);
        return view! { cx =>
            reset_password_form(user: &user, error: Some("Enter a new password."))
        }
        .single()
        .await;
    };
    application::users::reset_user_password::call(app_context::<AppState>(cx), user.id, password)
        .await?;
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
    application::users::delete_user::call(app_context::<AppState>(cx), user.id).await?;
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

#[route(POST "/admin/users/{user_id}/sessions/{session_id}/sign-out")]
async fn sign_out_device(cx: &Cx, Form(form): Form<ActionForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let user = find_user(cx).await?;
    let session_id = *path_param::<SessionId>(cx)?;
    let state = app_context::<AppState>(cx);
    let session = list_user_sessions::call(state, user.id)
        .await?
        .sessions
        .into_iter()
        .find(|x| x.id == session_id);
    let notice = match session {
        Some(session) => {
            application::sessions::end_session::call(state, session.id).await?;
            log_admin_action(AdminAction::SessionEnd, session.id);
            Notice::Done(format!(
                "Signed {} out of {}, {}.",
                user.name,
                client_kind_label(session.client_kind),
                session.device_name
            ))
        }
        None => Notice::Failed("This device is already signed out."),
    };
    view! { cx => user_section(user: &user, notice: Some(notice)) }
        .single()
        .await
}

#[route(POST "/admin/users/{user_id}/sign-out")]
async fn sign_out_everywhere(cx: &Cx, Form(form): Form<ActionForm>) -> topcoat::Result<ViewHandle> {
    require_admin(cx).await?;
    verify_csrf(cx, &form.csrf)?;
    let user = find_user(cx).await?;
    application::sessions::end_user_sessions::call(app_context::<AppState>(cx), user.id).await?;
    log_admin_action(AdminAction::SessionEndAll, user.id);
    let notice = Notice::Done(format!("Signed {} out everywhere.", user.name));
    view! { cx => user_section(user: &user, notice: Some(notice)) }
        .single()
        .await
}

async fn find_user(cx: &Cx) -> topcoat::Result<User> {
    let user_id = *path_param::<UserId>(cx)?;
    match get_user::call(app_context::<AppState>(cx), user_id).await? {
        get_user::Output::Found { id, name } => Ok(User { id, name }),
        get_user::Output::NotFound => Err(not_found().into()),
    }
}

async fn pending_invite_email(
    state: &AppState,
    invite_id: Uuid,
) -> topcoat::Result<Option<String>> {
    Ok(application::invites::list_pending_invites::call(state)
        .await?
        .invites
        .into_iter()
        .find(|x| x.id == invite_id)
        .map(|x| x.email))
}

fn client_kind_label(client_kind: ClientKind) -> &'static str {
    match client_kind {
        ClientKind::Web => "Web",
        ClientKind::AppleTv => "Apple TV",
        ClientKind::GoogleTv => "Google TV",
    }
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

fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|x| x.chars().next())
        .collect::<String>()
        .to_uppercase()
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
    let users = application::users::list_users::call(state).await?.users;
    let invites = application::invites::list_pending_invites::call(state)
        .await?
        .invites;
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
                        table_head(<span class="sr-only">"Colour"</span>)
                        table_head("Name")
                        table_head("Created")
                        table_head("Signed in on")
                        table_head("Last used")
                    )
                )
                table_body(
                    for user in &users {
                        table_row(
                            attrs: attributes! { class="relative" },
                            table_cell(
                                <span
                                    aria-hidden="true"
                                    class=(format!(
                                        "inline-flex size-8 items-center justify-center rounded-full text-sm font-semibold text-background {}",
                                        colour_class(user.colour),
                                    ))
                                >
                                    (initials(&user.name))
                                </span>
                            )
                            table_cell(
                                <a
                                    href=(format!("{USERS_PATH}/{}", user.id))
                                    class="font-medium after:absolute after:inset-0"
                                >
                                    (&user.name)
                                </a>
                            )
                            table_cell(local_time(at: user.created_at, format: TimeFormat::Absolute))
                            table_cell((user.signed_in_devices.to_string()))
                            table_cell(
                                if let Some(last_used_at) = user.last_used_at {
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
                            table_head("Email")
                            table_head(<span class="sr-only">"Actions"</span>)
                        )
                    )
                    table_body(
                        for invite in &invites {
                            table_row(
                                table_cell((&invite.email))
                                table_cell(
                                    match invite.email_status {
                                        EmailStatus::Queued { queued_at } => {
                                            "Sending since "
                                            local_time(at: queued_at, format: TimeFormat::Relative)
                                        }
                                        EmailStatus::Sent { sent_at } => {
                                            "Sent "
                                            local_time(at: sent_at, format: TimeFormat::Absolute)
                                            if invite.expired {
                                                ", "
                                                badge(variant: BadgeVariant::Destructive, "Expired")
                                            } else if let Some(expires_at) = invite.expires_at {
                                                ", expires "
                                                local_time(at: expires_at, format: TimeFormat::Absolute)
                                            }
                                        }
                                        EmailStatus::Refused { .. } => {
                                            badge(
                                                variant: BadgeVariant::Destructive,
                                                "The mail server refused this address"
                                            )
                                        }
                                    }
                                )
                                table_cell(
                                    <div class="flex items-center justify-end gap-2">
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
async fn user_section(cx: &Cx, user: &User, notice: Option<Notice>) -> topcoat::Result<impl View> {
    let sessions = list_user_sessions::call(app_context::<AppState>(cx), user.id).await?;
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
            signed_in_on(user: user, sessions: &sessions)
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
async fn signed_in_on(
    user: &User,
    sessions: &list_user_sessions::Output,
) -> topcoat::Result<impl View> {
    let sessions = &sessions.sessions;
    Ok(view! {
        <section class="flex flex-col gap-4">
            <div class="flex items-center justify-between gap-4">
                <h2 class="text-base font-semibold">"Signed in on"</h2>
                if !sessions.is_empty() {
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            commandfor=(SIGN_OUT_EVERYWHERE_DIALOG_ID)
                            command="show-modal"
                        },
                        "Sign out everywhere"
                    )
                }
            </div>
            if sessions.is_empty() {
                <p class="text-sm text-muted-foreground">"Not signed in on any device."</p>
            } else {
                table(
                    table_header(
                        table_row(
                            table_head("Client")
                            table_head("Device")
                            table_head("Signed in")
                            table_head("Last used")
                            table_head(<span class="sr-only">"Actions"</span>)
                        )
                    )
                    table_body(
                        for session in sessions {
                            table_row(
                                table_cell((client_kind_label(session.client_kind)))
                                table_cell((&session.device_name))
                                table_cell(local_time(at: session.signed_in_at, format: TimeFormat::Absolute))
                                table_cell(
                                    local_time(
                                        at: session.last_used_at,
                                        format: TimeFormat::Relative,
                                        hour_precision: true,
                                    )
                                )
                                table_cell(
                                    <div class="flex justify-end">
                                        button(
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Sm,
                                            attrs: attributes! {
                                                type="button"
                                                commandfor=(format!("sign-out-{}", session.id))
                                                command="show-modal"
                                            },
                                            "Sign out"
                                        )
                                    </div>
                                )
                            )
                        }
                    )
                )
            }
            for session in sessions {
                alert_dialog(
                    open: false,
                    attrs: attributes! { id=(format!("sign-out-{}", session.id)) },
                    dialog_content(
                        dialog_header(
                            dialog_title((format!(
                                "Sign {} out of {}, {}?",
                                user.name,
                                client_kind_label(session.client_kind),
                                session.device_name,
                            )))
                            dialog_description("They need their password to sign in on this device again.")
                        )
                        sign_out_form(
                            action: format!("{USERS_PATH}/{}/sessions/{}/sign-out", user.id, session.id),
                            dialog_id: format!("sign-out-{}", session.id),
                            label: "Sign out",
                        )
                    )
                )
            }
            alert_dialog(
                open: false,
                attrs: attributes! { id=(SIGN_OUT_EVERYWHERE_DIALOG_ID) },
                dialog_content(
                    dialog_header(
                        dialog_title((format!("Sign {} out everywhere?", user.name)))
                        dialog_description("They need their password to sign in on each device again.")
                    )
                    sign_out_form(
                        action: format!("{USERS_PATH}/{}/sign-out", user.id),
                        dialog_id: SIGN_OUT_EVERYWHERE_DIALOG_ID.to_owned(),
                        label: "Sign out everywhere",
                    )
                )
            )
        </section>
    })
}

#[component]
async fn sign_out_form(
    action: String,
    dialog_id: String,
    label: &'static str,
) -> topcoat::Result<impl View> {
    Ok(view! {
        <form
            method="post"
            action=(&action)
            hx-post=(&action)
            hx-target=(format!("#{USER_ID}"))
            hx-swap=(SWAP)
        >
            csrf_field()
            dialog_footer(
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! { type="button" commandfor=(&dialog_id) command="close" },
                    "Cancel"
                )
                button(
                    variant: ButtonVariant::Destructive,
                    attrs: attributes! { type="submit" },
                    (label)
                )
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

#[cfg(test)]
mod tests {
    use super::colour_class;
    use crate::test::ADMIN_NAME;
    use crate::test::ADMIN_PASSWORD;
    use crate::test::Logs;
    use crate::test::USER_NAME as NAME;
    use crate::test::USER_PASSWORD as PASSWORD;
    use crate::test::create_admin;
    use crate::test::create_user;
    use crate::test::current_user_name;
    use crate::test::send_invite_emails;
    use crate::test::serve;
    use crate::test::signed_in;
    use crate::test::user_signs_in;
    use application::invites::accept_invite;
    use application::ports::EmailRefused;
    use application::ports::SendEmail;
    use application::ports::SendEmailError;
    use domain::ClientKind;
    use domain::Password;
    use http::StatusCode;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use test_app::TestApp;
    use test_fixtures::Browser;
    use test_fixtures::Page;
    use test_fixtures::TestServer;
    use uuid::Uuid;

    const EMAIL: &str = "ravi@household.example";
    const NEW_PASSWORD: &str = "a new passphrase about coffee";
    const WEEK: SignedDuration = SignedDuration::from_hours(7 * 24);

    async fn start() -> (TestApp, TestServer) {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        (app, server)
    }

    async fn admin(server: &TestServer) -> Browser {
        signed_in(server, ADMIN_NAME, ADMIN_PASSWORD).await
    }

    async fn submit(
        browser: &mut Browser,
        page: &Page,
        button: &str,
        fields: &[(&str, &str)],
    ) -> Page {
        let csrf = page.csrf_token();
        let mut form = vec![("csrf", csrf.as_str())];
        form.extend_from_slice(fields);
        browser.post_form(&page.form_action(button), &form).await
    }

    async fn invite(browser: &mut Browser, email: &str) -> Page {
        let users = browser.get("/admin/users").await;
        submit(browser, &users, "Send invite", &[("email", email)]).await
    }

    async fn user_page(browser: &mut Browser, name: &str) -> Page {
        let users = browser.get("/admin/users").await;
        browser.get(&users.link(name)).await
    }

    fn utc(at: Timestamp) -> String {
        at.strftime("%Y-%m-%d %H:%M UTC").to_string()
    }

    fn rows_with(page: &Page, text: &str) -> Vec<Vec<String>> {
        page.table_rows()
            .into_iter()
            .filter(|x| x.iter().any(|x| x == text))
            .collect()
    }

    fn token_from_last_email(app: &TestApp, server: &TestServer) -> String {
        let body = app.mailer.sent().last().unwrap().body.clone();
        let link = format!("{}/invite#", server.base_url());
        let start = body.find(&link).unwrap_or_else(|| {
            panic!("the email has no invite link on the admin's origin\n{body}")
        }) + link.len();
        body[start..].split_whitespace().next().unwrap().to_owned()
    }

    async fn accepts(app: &TestApp, token: &str) -> bool {
        accept_invite::call(
            &app.state,
            token,
            "Ravi".to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            Uuid::new_v4(),
        )
        .await
        .is_ok()
    }

    async fn sign_in(app: &TestApp, password: &str, client_kind: ClientKind) -> String {
        sign_in_on(app, password, client_kind, "Living Room").await
    }

    async fn sign_in_on(
        app: &TestApp,
        password: &str,
        client_kind: ClientKind,
        device_name: &str,
    ) -> String {
        user_signs_in(app, NAME, password, client_kind, device_name)
            .await
            .expect("the user signs in")
    }

    async fn confirm(browser: &mut Browser, page: &Page, dialog_title: &str) -> Page {
        let title_at = page
            .body
            .find(&format!(">{dialog_title}<"))
            .unwrap_or_else(|| panic!("the page has no {dialog_title:?} dialog\n{}", page.body));
        let start =
            title_at + page.body[title_at..].find(r#"action=""#).unwrap() + r#"action=""#.len();
        let length = page.body[start..].find('"').unwrap();
        let action = page.body[start..start + length].to_owned();
        browser
            .post_form(&action, &[("csrf", &page.csrf_token())])
            .await
    }

    #[tokio::test]
    async fn users_lists_each_user_with_their_initials_created_time_signed_in_devices_and_last_use()
    {
        let (app, server) = start().await;
        let created_at = app.clock.now();
        create_user(&app, NAME).await;
        sign_in(&app, PASSWORD, ClientKind::Web).await;
        let tv = sign_in(&app, PASSWORD, ClientKind::AppleTv).await;
        app.clock.advance(SignedDuration::from_hours(31 * 24));
        let used_at = app.clock.now();
        current_user_name(&app, &tv).await;
        create_user(&app, "Ravi Shah").await;
        let mut browser = admin(&server).await;

        let users = browser.get("/admin/users").await;

        assert_eq!(users.status, StatusCode::OK);
        assert_eq!(users.link("Users"), "/admin/users");
        assert_eq!(
            rows_with(&users, NAME),
            [vec![
                "N".to_owned(),
                NAME.to_owned(),
                utc(created_at),
                "1".to_owned(),
                utc(used_at),
            ]]
        );
        assert_eq!(
            rows_with(&users, "Ravi Shah"),
            [vec![
                "RS".to_owned(),
                "Ravi Shah".to_owned(),
                utc(used_at),
                "0".to_owned(),
                "Never".to_owned(),
            ]]
        );
    }

    #[tokio::test]
    async fn each_user_shows_the_colour_that_comes_from_their_id() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let colour = match application::sessions::sign_in::call(
            &app.state,
            NAME.to_owned(),
            PASSWORD.to_owned(),
            ClientKind::Web,
            "Firefox".to_owned(),
            Uuid::new_v4(),
        )
        .await
        .unwrap()
        {
            application::sessions::sign_in::Output::SignedIn { colour, .. } => colour,
            application::sessions::sign_in::Output::WrongNameOrPassword => {
                panic!("the user signs in")
            }
        };
        let mut browser = admin(&server).await;

        let users = browser.get("/admin/users").await;

        let row = users
            .body
            .split("<tr")
            .find(|x| x.contains(&format!(">{NAME}<")))
            .unwrap();
        assert!(row.contains(colour_class(colour)), "{row}");
    }

    #[tokio::test]
    async fn inviting_an_address_queues_an_email_with_a_link_on_the_admins_origin() {
        let (app, server) = start().await;
        let mut browser = admin(&server).await;
        let queued_at = app.clock.now();

        let invited = invite(&mut browser, EMAIL).await;
        app.clock.advance(SignedDuration::from_mins(1));
        let sent_at = app.clock.now();
        send_invite_emails(&app).await;
        let users = browser.get("/admin/users").await;

        assert_eq!(invited.status, StatusCode::OK);
        assert!(
            invited
                .text()
                .contains(&format!("Sending an invite to {EMAIL}.")),
            "{}",
            invited.body
        );
        assert_eq!(
            rows_with(&invited, EMAIL)[0][1],
            format!("Sending since {}", utc(queued_at))
        );
        let sent = app.mailer.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].recipient, EMAIL);
        assert_eq!(
            rows_with(&users, EMAIL)[0][1],
            format!("Sent {}, expires {}", utc(sent_at), utc(sent_at + WEEK))
        );
        assert!(accepts(&app, &token_from_last_email(&app, &server)).await);
    }

    #[tokio::test]
    async fn an_address_that_already_has_a_user_or_a_pending_invite_shows_an_error() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;

        let user = invite(&mut browser, "noor@example.com").await;
        let pending = invite(&mut browser, EMAIL).await;
        send_invite_emails(&app).await;

        for page in [user, pending] {
            assert_eq!(page.status, StatusCode::OK);
            assert!(
                page.text()
                    .contains("A user or a pending invite already has this address."),
                "{}",
                page.body
            );
        }
        assert_eq!(app.mailer.sent().len(), 1);
    }

    #[tokio::test]
    async fn an_address_the_mail_server_refused_shows_as_refused() {
        let (mut app, _) = start().await;
        app.state.send_email = SendEmail::new(|_, _, _| {
            Box::pin(async { Err(SendEmailError::caused_by(EmailRefused::new())) })
        });
        let server = serve(&app).await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;

        send_invite_emails(&app).await;
        let users = browser.get("/admin/users").await;

        assert_eq!(
            rows_with(&users, EMAIL)[0][1],
            "The mail server refused this address"
        );
    }

    #[tokio::test]
    async fn after_a_resend_the_old_link_works_until_the_new_email_goes_out() {
        let (app, server) = start().await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;
        send_invite_emails(&app).await;
        let old = token_from_last_email(&app, &server);
        let users = browser.get("/admin/users").await;

        let resent = submit(&mut browser, &users, "Resend", &[]).await;

        assert!(
            resent
                .text()
                .contains(&format!("Sending a new invite to {EMAIL}.")),
            "{}",
            resent.body
        );
        assert!(accepts(&app, &old).await);
    }

    #[tokio::test]
    async fn once_the_resent_email_goes_out_only_the_new_link_works() {
        let (app, server) = start().await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;
        send_invite_emails(&app).await;
        let old = token_from_last_email(&app, &server);
        app.clock.advance(SignedDuration::from_hours(24));
        let mut browser = admin(&server).await;
        let users = browser.get("/admin/users").await;
        submit(&mut browser, &users, "Resend", &[]).await;
        let resent_at = app.clock.now();

        send_invite_emails(&app).await;
        let users = browser.get("/admin/users").await;

        assert_eq!(
            rows_with(&users, EMAIL)[0][1],
            format!("Sent {}, expires {}", utc(resent_at), utc(resent_at + WEEK))
        );
        assert_eq!(app.mailer.sent().len(), 2);
        let new = token_from_last_email(&app, &server);
        assert!(!accepts(&app, &old).await);
        assert!(accepts(&app, &new).await);
    }

    #[tokio::test]
    async fn cancelling_an_invite_removes_it_and_its_link_stops_working() {
        let (app, server) = start().await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;
        send_invite_emails(&app).await;
        let token = token_from_last_email(&app, &server);
        let users = browser.get("/admin/users").await;

        let cancelled = submit(&mut browser, &users, "Cancel invite", &[]).await;

        assert!(
            cancelled
                .text()
                .contains(&format!("Cancelled the invite to {EMAIL}.")),
            "{}",
            cancelled.body
        );
        assert_eq!(rows_with(&cancelled, EMAIL), Vec::<Vec<String>>::new());
        assert_eq!(
            rows_with(&browser.get("/admin/users").await, EMAIL),
            Vec::<Vec<String>>::new()
        );
        assert!(!accepts(&app, &token).await);
    }

    #[tokio::test]
    async fn an_invite_shows_as_expired_7_days_after_its_email_was_sent() {
        let (app, server) = start().await;
        let mut browser = admin(&server).await;
        invite(&mut browser, EMAIL).await;
        send_invite_emails(&app).await;

        app.clock.advance(WEEK - SignedDuration::from_secs(1));
        let before = admin(&server).await.get("/admin/users").await;
        app.clock.advance(SignedDuration::from_secs(1));
        let after = admin(&server).await.get("/admin/users").await;

        assert!(!rows_with(&before, EMAIL)[0].concat().contains("Expired"));
        assert!(rows_with(&after, EMAIL)[0].concat().contains("Expired"));
    }

    #[tokio::test]
    async fn a_users_row_opens_their_page_with_their_name_and_actions() {
        let (app, server) = start().await;
        let id = create_user(&app, NAME).await;
        let mut browser = admin(&server).await;

        let users = browser.get("/admin/users").await;
        let user = browser.get(&users.link(NAME)).await;
        let unknown = browser
            .get("/admin/users/01900000-0000-7000-8000-000000000000")
            .await;

        assert_eq!(users.link(NAME), format!("/admin/users/{id}"));
        assert_eq!(user.status, StatusCode::OK);
        let text = user.text();
        for expected in [NAME, "Reset password", "Delete user"] {
            assert!(
                text.contains(expected),
                "{expected:?} is missing\n{}",
                user.body
            );
        }
        assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn resetting_a_password_signs_the_user_out_everywhere() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let web = sign_in(&app, PASSWORD, ClientKind::Web).await;
        let tv = sign_in(&app, PASSWORD, ClientKind::AppleTv).await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        let reset = submit(&mut browser, &user, "Reset", &[("password", NEW_PASSWORD)]).await;

        assert!(
            reset.text().contains(&format!(
                "Reset {NAME}'s password and signed them out everywhere."
            )),
            "{}",
            reset.body
        );
        assert_eq!(current_user_name(&app, &web).await, None);
        assert_eq!(current_user_name(&app, &tv).await, None);
        assert_eq!(
            user_signs_in(&app, NAME, PASSWORD, ClientKind::Web, "Firefox").await,
            None
        );
        sign_in(&app, NEW_PASSWORD, ClientKind::Web).await;
    }

    #[tokio::test]
    async fn a_user_is_deleted_only_when_the_typed_name_matches_in_any_capitals() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let token = sign_in(&app, PASSWORD, ClientKind::Web).await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        let mistyped = submit(&mut browser, &user, "Delete", &[("name", "Nor")]).await;
        let kept = browser.get("/admin/users").await;
        let deleted = submit(&mut browser, &user, "Delete", &[("name", "nOOR")]).await;
        let users = browser.get("/admin/users").await;

        assert!(
            mistyped.text().contains("The name does not match."),
            "{}",
            mistyped.body
        );
        assert_eq!(rows_with(&kept, NAME).len(), 1);
        assert!(
            deleted
                .text()
                .contains(&format!("Deleted {NAME} and their watch history.")),
            "{}",
            deleted.body
        );
        assert_eq!(rows_with(&users, NAME), Vec::<Vec<String>>::new());
        assert_eq!(current_user_name(&app, &token).await, None);
    }

    #[tokio::test]
    async fn renaming_a_user_keeps_them_signed_in_and_their_apps_get_the_new_name() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let web = sign_in(&app, PASSWORD, ClientKind::Web).await;
        let tv = sign_in(&app, PASSWORD, ClientKind::AppleTv).await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        let renamed = submit(&mut browser, &user, "Save", &[("name", "Nour")]).await;
        let users = browser.get("/admin/users").await;

        assert!(user.text().contains("Rename"), "{}", user.body);
        assert!(
            renamed.text().contains("Renamed Noor to Nour."),
            "{}",
            renamed.body
        );
        assert_eq!(rows_with(&users, "Nour").len(), 1);
        assert_eq!(rows_with(&users, NAME), Vec::<Vec<String>>::new());
        for token in [&web, &tv] {
            assert_eq!(
                current_user_name(&app, token).await.as_deref(),
                Some("Nour")
            );
        }
    }

    #[tokio::test]
    async fn a_user_can_be_renamed_to_their_name_in_other_capitals() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        submit(&mut browser, &user, "Save", &[("name", "NOOR")]).await;

        let users = browser.get("/admin/users").await;
        assert_eq!(rows_with(&users, "NOOR").len(), 1);
    }

    #[tokio::test]
    async fn a_rename_to_a_taken_or_empty_name_shows_an_error_and_keeps_the_name() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        create_user(&app, "Ravi").await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, "Ravi").await;

        let taken = submit(&mut browser, &user, "Save", &[("name", NAME)]).await;
        let empty = submit(&mut browser, &user, "Save", &[("name", "")]).await;
        let users = browser.get("/admin/users").await;

        assert!(
            taken.text().contains("A user already has this name."),
            "{}",
            taken.body
        );
        assert!(empty.text().contains("Enter a name."), "{}", empty.body);
        assert_eq!(rows_with(&users, "Ravi").len(), 1);
        assert_eq!(rows_with(&users, NAME).len(), 1);
    }

    #[tokio::test]
    async fn a_users_page_lists_the_devices_they_are_signed_in_on() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        sign_in_on(&app, PASSWORD, ClientKind::Web, "Firefox on macOS").await;
        app.clock.advance(SignedDuration::from_hours(31 * 24));
        let tv_signed_in_at = app.clock.now();
        let tv = sign_in_on(&app, PASSWORD, ClientKind::AppleTv, "Living Room").await;
        app.clock.advance(SignedDuration::from_hours(2));
        let google_tv_signed_in_at = app.clock.now();
        sign_in_on(&app, PASSWORD, ClientKind::GoogleTv, "Bedroom").await;
        app.clock.advance(SignedDuration::from_hours(3));
        let tv_used_at = app.clock.now();
        current_user_name(&app, &tv).await;
        let mut browser = admin(&server).await;

        let user = user_page(&mut browser, NAME).await;

        assert_eq!(
            user.table_rows(),
            [
                vec![
                    "Apple TV".to_owned(),
                    "Living Room".to_owned(),
                    utc(tv_signed_in_at),
                    utc(tv_used_at),
                    "Sign out".to_owned(),
                ],
                vec![
                    "Google TV".to_owned(),
                    "Bedroom".to_owned(),
                    utc(google_tv_signed_in_at),
                    utc(google_tv_signed_in_at),
                    "Sign out".to_owned(),
                ],
            ]
        );
        let text = user.text();
        for expected in ["Signed in on", "Sign out everywhere"] {
            assert!(
                text.contains(expected),
                "{expected:?} is missing\n{}",
                user.body
            );
        }
        assert!(!text.to_lowercase().contains("session"), "{text}");
    }

    #[tokio::test]
    async fn signing_out_one_device_ends_only_that_sign_in() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        let living_room = sign_in_on(&app, PASSWORD, ClientKind::AppleTv, "Living Room").await;
        let bedroom = sign_in_on(&app, PASSWORD, ClientKind::GoogleTv, "Bedroom").await;
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        let signed_out = confirm(
            &mut browser,
            &user,
            &format!("Sign {NAME} out of Apple TV, Living Room?"),
        )
        .await;
        let again = confirm(
            &mut browser,
            &user,
            &format!("Sign {NAME} out of Apple TV, Living Room?"),
        )
        .await;

        assert_eq!(signed_out.status, StatusCode::OK);
        assert!(
            signed_out
                .text()
                .contains(&format!("Signed {NAME} out of Apple TV, Living Room.")),
            "{}",
            signed_out.body
        );
        assert_eq!(
            signed_out
                .table_rows()
                .into_iter()
                .map(|x| x[1].clone())
                .collect::<Vec<_>>(),
            ["Bedroom"]
        );
        assert!(
            again.text().contains("This device is already signed out."),
            "{}",
            again.body
        );
        assert_eq!(current_user_name(&app, &living_room).await, None);
        assert_eq!(
            current_user_name(&app, &bedroom).await.as_deref(),
            Some(NAME)
        );
    }

    #[tokio::test]
    async fn signing_out_everywhere_ends_every_sign_in_of_that_user_only() {
        let (app, server) = start().await;
        create_user(&app, NAME).await;
        create_user(&app, "Ravi").await;
        let web = sign_in_on(&app, PASSWORD, ClientKind::Web, "Firefox on macOS").await;
        let tv = sign_in_on(&app, PASSWORD, ClientKind::AppleTv, "Living Room").await;
        let ravi = user_signs_in(&app, "Ravi", PASSWORD, ClientKind::AppleTv, "Den")
            .await
            .unwrap();
        let mut browser = admin(&server).await;
        let user = user_page(&mut browser, NAME).await;

        let signed_out =
            confirm(&mut browser, &user, &format!("Sign {NAME} out everywhere?")).await;

        assert_eq!(signed_out.status, StatusCode::OK);
        assert!(
            signed_out
                .text()
                .contains(&format!("Signed {NAME} out everywhere.")),
            "{}",
            signed_out.body
        );
        assert_eq!(signed_out.table_rows(), Vec::<Vec<String>>::new());
        assert_eq!(current_user_name(&app, &web).await, None);
        assert_eq!(current_user_name(&app, &tv).await, None);
        assert_eq!(
            current_user_name(&app, &ravi).await.as_deref(),
            Some("Ravi")
        );
    }

    #[tokio::test]
    async fn each_users_page_action_logs_its_action_and_target_without_the_email_address() {
        let (app, server) = start().await;
        let user_id = create_user(&app, NAME).await;
        let mut browser = admin(&server).await;
        let logs = Logs::capture();

        let users = browser.get("/admin/users").await;
        let csrf = users.csrf_token();
        let invited = browser
            .post_form(
                &users.form_action("Send invite"),
                &[("csrf", &csrf), ("email", EMAIL)],
            )
            .await;
        let resend = invited.form_action("Resend");
        let invite_id = resend.split('/').nth(4).unwrap().to_owned();
        browser.post_form(&resend, &[("csrf", &csrf)]).await;
        browser
            .post_form(&invited.form_action("Cancel invite"), &[("csrf", &csrf)])
            .await;
        let user = browser.get(&users.link(NAME)).await;
        browser
            .post_form(
                &user.form_action("Reset"),
                &[("csrf", &csrf), ("password", NEW_PASSWORD)],
            )
            .await;
        browser
            .post_form(
                &user.form_action("Delete"),
                &[("csrf", &csrf), ("name", NAME)],
            )
            .await;

        let user_id = user_id.to_string();
        assert_eq!(
            logs.admin_actions(),
            [
                ("invite.create", invite_id.clone()),
                ("invite.resend", invite_id.clone()),
                ("invite.cancel", invite_id),
                ("user.reset_password", user_id.clone()),
                ("user.delete", user_id),
            ]
            .map(|(action, target)| (action.to_owned(), target))
        );
        logs.assert_none_contains(&[EMAIL, NEW_PASSWORD]);
    }

    #[tokio::test]
    async fn renaming_a_user_logs_the_action_and_target_without_either_name() {
        let (app, server) = start().await;
        let user_id = create_user(&app, NAME).await;
        let mut browser = admin(&server).await;
        let logs = Logs::capture();

        let user = browser.get(&format!("/admin/users/{user_id}")).await;
        browser
            .post_form(
                &user.form_action("Save"),
                &[("csrf", &user.csrf_token()), ("name", "Nour")],
            )
            .await;

        assert_eq!(
            logs.admin_actions(),
            [("user.rename".to_owned(), user_id.to_string())]
        );
        logs.assert_none_contains(&[NAME, "Nour"]);
    }

    #[tokio::test]
    async fn signing_a_user_out_logs_the_device_or_the_user_as_the_target() {
        let (app, server) = start().await;
        let user_id = create_user(&app, NAME).await;
        sign_in(&app, PASSWORD, ClientKind::AppleTv).await;
        let mut browser = admin(&server).await;
        let logs = Logs::capture();

        let user = browser.get(&format!("/admin/users/{user_id}")).await;
        let csrf = user.csrf_token();
        let sign_out_device = user
            .body
            .split(r#"action=""#)
            .map(|x| &x[..x.find('"').unwrap()])
            .find(|x| x.contains("/sessions/"))
            .unwrap()
            .to_owned();
        let session_id = sign_out_device.split('/').nth(5).unwrap().to_owned();
        browser
            .post_form(&sign_out_device, &[("csrf", &csrf)])
            .await;
        browser
            .post_form(
                &format!("/admin/users/{user_id}/sign-out"),
                &[("csrf", &csrf)],
            )
            .await;

        assert_eq!(
            logs.admin_actions(),
            [
                ("session.end".to_owned(), session_id),
                ("session.end_all".to_owned(), user_id.to_string()),
            ]
        );
    }
}
