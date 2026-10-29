use crate::ACCOUNT_PASSWORD_PATH;
use crate::ADMINS_PATH;
use crate::AdminAction;
use crate::SESSION_COOKIE;
use crate::SIGN_IN_PATH;
use crate::admin_cookie;
use crate::admin_page;
use crate::components::alert::AlertVariant;
use crate::components::alert::alert;
use crate::components::alert::alert_title;
use crate::components::alert_dialog::alert_dialog;
use crate::components::button::ButtonSize;
use crate::components::button::ButtonVariant;
use crate::components::button::button;
use crate::components::button::button_variants;
use crate::components::dialog::dialog;
use crate::components::dialog::dialog_content;
use crate::components::dialog::dialog_description;
use crate::components::dialog::dialog_footer;
use crate::components::dialog::dialog_header;
use crate::components::dialog::dialog_title;
use crate::components::field::field;
use crate::components::field::field_error;
use crate::components::field::field_group;
use crate::components::field::field_label;
use crate::components::input::input;
use crate::components::table::table;
use crate::components::table::table_body;
use crate::components::table::table_cell;
use crate::components::table::table_head;
use crate::components::table::table_header;
use crate::components::table::table_row;
use crate::csrf_field;
use crate::local_time::TimeFormat;
use crate::local_time::local_time;
use crate::log_admin_action;
use crate::require_admin;
use crate::verify_csrf;
use application::EmptyPassword;
use application::change_password::WrongCurrentPassword;
use application::create_admin::NameTaken;
use application::delete_admin::LastAdmin;
use infrastructure::AppState;
use ohno::ErrorExt;
use serde::Deserialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::cookie::Cookies;
use topcoat::cookie::cookies;
use topcoat::router::RouterBuilder;
use topcoat::router::content::Form;
use topcoat::router::error::forbidden;
use topcoat::router::error::see_other;
use topcoat::router::page;
use topcoat::router::path_param;
use topcoat::view::Child;
use topcoat::view::View;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::view;
use uuid::Uuid;

path_param!(admin_id: Uuid, error = not_found);

pub(crate) fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .page(list_admins)
        .page(create_admin)
        .page(set_password)
        .page(delete_admin)
        .page(account_password)
        .page(change_password)
}

const ADMINS_ID: &str = "admins";
const ACCOUNT_PASSWORD_ID: &str = "account-password";
const NEW_ADMIN_ID: &str = "new-admin";
const ENTER_A_PASSWORD: &str = "Enter a password.";

enum Outcome {
    None,
    Done(&'static str),
    Failed(&'static str),
    NewAdminInvalid {
        name: String,
        name_error: Option<&'static str>,
        password_error: Option<&'static str>,
    },
    SetPasswordInvalid {
        admin_id: Uuid,
        error: &'static str,
    },
}

#[page("/admin/admins")]
async fn list_admins(cx: &Cx) -> topcoat::Result<impl View> {
    let me = require_admin(cx).await?;
    Ok(view! { admins_page(me: me, outcome: Outcome::None) })
}

#[derive(Deserialize)]
struct NewAdmin {
    #[serde(default)]
    csrf: String,
    name: String,
    password: String,
}

#[page(POST "/admin/admins")]
async fn create_admin(cx: &Cx, Form(form): Form<NewAdmin>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let me = require_admin(cx).await?;
    let outcome = match application::create_admin::call(
        app_context::<AppState>(cx),
        form.name.clone(),
        form.password,
    )
    .await
    {
        Ok(admin) => {
            log_admin_action(AdminAction::AdminCreate, admin.id);
            Outcome::Done("Admin created.")
        }
        Err(error) if error.find_source::<NameTaken>().is_some() => Outcome::NewAdminInvalid {
            name: form.name,
            name_error: Some("An admin already has this name."),
            password_error: None,
        },
        Err(error) if error.find_source::<EmptyPassword>().is_some() => Outcome::NewAdminInvalid {
            name: form.name,
            name_error: None,
            password_error: Some(ENTER_A_PASSWORD),
        },
        Err(error) => return Err(error.into()),
    };
    Ok(view! { admins_page(me: me, outcome: outcome) })
}

#[derive(Deserialize)]
struct SetPassword {
    #[serde(default)]
    csrf: String,
    password: String,
}

#[page(POST "/admin/admins/{admin_id}/password")]
async fn set_password(cx: &Cx, Form(form): Form<SetPassword>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let me = require_admin(cx).await?;
    let admin_id = *path_param::<AdminId>(cx)?;
    if admin_id == me {
        return Err(forbidden().into());
    }
    let outcome = match application::change_admin_password::call(
        app_context::<AppState>(cx),
        admin_id,
        form.password,
    )
    .await
    {
        Ok(()) => {
            log_admin_action(AdminAction::AdminSetPassword, admin_id);
            Outcome::Done("Password set.")
        }
        Err(error) if error.find_source::<EmptyPassword>().is_some() => {
            Outcome::SetPasswordInvalid {
                admin_id,
                error: ENTER_A_PASSWORD,
            }
        }
        Err(error) => return Err(error.into()),
    };
    Ok(view! { admins_page(me: me, outcome: outcome) })
}

#[derive(Deserialize)]
struct DeleteAdmin {
    #[serde(default)]
    csrf: String,
}

#[page(POST "/admin/admins/{admin_id}/delete")]
async fn delete_admin(cx: &Cx, Form(form): Form<DeleteAdmin>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let me = require_admin(cx).await?;
    let admin_id = *path_param::<AdminId>(cx)?;
    let outcome = match application::delete_admin::call(app_context::<AppState>(cx), admin_id).await
    {
        Ok(()) => {
            log_admin_action(AdminAction::AdminDelete, admin_id);
            if admin_id == me {
                cookies(cx).remove(admin_cookie(SESSION_COOKIE, String::new()));
                return Err(see_other(SIGN_IN_PATH).into());
            }
            Outcome::Done("Admin deleted.")
        }
        Err(error) if error.find_source::<LastAdmin>().is_some() => {
            Outcome::Failed("The last admin can't be deleted.")
        }
        Err(error) => return Err(error.into()),
    };
    Ok(view! { admins_page(me: me, outcome: outcome) })
}

#[component]
async fn admins_page(cx: &Cx, me: Uuid, outcome: Outcome) -> topcoat::Result<impl View> {
    let admins = application::list_admins::call(app_context::<AppState>(cx)).await?;
    let mut notice = None;
    let mut new_admin = None;
    let mut set_password_error = None;
    match outcome {
        Outcome::None => {}
        Outcome::Done(message) => notice = Some((AlertVariant::Neutral, message)),
        Outcome::Failed(message) => notice = Some((AlertVariant::Destructive, message)),
        Outcome::NewAdminInvalid {
            name,
            name_error,
            password_error,
        } => new_admin = Some((name, name_error, password_error)),
        Outcome::SetPasswordInvalid { admin_id, error } => {
            set_password_error = Some((admin_id, error));
        }
    }
    let (new_admin_name, name_error, password_error) = new_admin.unwrap_or_default();
    let new_admin_open = name_error.is_some() || password_error.is_some();
    Ok(view! {
        admin_page(
            path: ADMINS_PATH,
            title: "Admins",
            <div id=(ADMINS_ID) hx-target="this" hx-select=(format!("#{ADMINS_ID}")) hx-swap="outerHTML" class="flex flex-col gap-4">
                if let Some((variant, message)) = notice {
                    alert(variant: variant, alert_title((message)))
                }
                <div class="flex justify-end">
                    button(
                        attrs: attributes! { type="button" commandfor=(NEW_ADMIN_ID) command="show-modal" },
                        "New admin"
                    )
                </div>
                table(
                    table_header(
                        table_row(
                            table_head("Name")
                            table_head("Created")
                            table_head("Last sign-in")
                            table_head(<span class="sr-only">"Actions"</span>)
                        )
                    )
                    table_body(
                        for admin in &admins {
                            table_row(
                                table_cell((admin.name.as_str()))
                                table_cell(local_time(at: admin.created_at, format: TimeFormat::Absolute))
                                table_cell(
                                    if let Some(at) = admin.last_signed_in_at {
                                        local_time(at: at, format: TimeFormat::Relative)
                                    } else {
                                        "Never"
                                    }
                                )
                                table_cell(
                                    <div class="flex justify-end gap-2">
                                        if admin.id == me {
                                            <a
                                                href=(ACCOUNT_PASSWORD_PATH)
                                                class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))
                                            >
                                                "Change my password"
                                            </a>
                                        } else {
                                            button(
                                                variant: ButtonVariant::Outline,
                                                size: ButtonSize::Sm,
                                                attrs: attributes! {
                                                    type="button"
                                                    commandfor=(set_password_id(admin.id))
                                                    command="show-modal"
                                                },
                                                "Set password"
                                            )
                                        }
                                        button(
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Sm,
                                            attrs: attributes! {
                                                type="button"
                                                class="text-destructive"
                                                commandfor=(delete_id(admin.id))
                                                command="show-modal"
                                            },
                                            "Delete"
                                        )
                                    </div>
                                )
                            )
                        }
                    )
                )
                dialog(
                    open: new_admin_open,
                    attrs: attributes! { id=(NEW_ADMIN_ID) aria-labelledby="new-admin-title" },
                    dialog_content(
                        swap_form(
                            action: ADMINS_PATH,
                            dialog_header(dialog_title(attrs: attributes! { id="new-admin-title" }, "New admin"))
                            field_group(
                                form_field(
                                    id: "new-admin-name",
                                    label: "Name",
                                    name: "name",
                                    kind: "text",
                                    autocomplete: "off",
                                    value: &new_admin_name,
                                    error: name_error
                                )
                                form_field(
                                    id: "new-admin-password",
                                    label: "Password",
                                    name: "password",
                                    kind: "password",
                                    autocomplete: "new-password",
                                    error: password_error
                                )
                            )
                            dialog_footer(
                                cancel_button(dialog_id: NEW_ADMIN_ID)
                                button(attrs: attributes! { type="submit" }, "Create admin")
                            )
                        )
                    )
                )
                for admin in admins.iter().filter(|x| x.id != me) {
                    let dialog_id = set_password_id(admin.id);
                    let action = format!("{ADMINS_PATH}/{}/password", admin.id);
                    let input_id = format!("password-{}", admin.id);
                    let error = set_password_error
                        .filter(|(admin_id, _)| *admin_id == admin.id)
                        .map(|(_, error)| error);
                    dialog(
                        open: error.is_some(),
                        attrs: attributes! { id=(dialog_id.as_str()) aria-label=(format!("Set {}'s password", admin.name)) },
                        dialog_content(
                            swap_form(
                                action: &action,
                                dialog_header(
                                    dialog_title(
                                        "Set " (admin.name.as_str()) "'s password"
                                    )
                                    dialog_description(
                                        (admin.name.as_str()) " is signed out of the admin app everywhere."
                                    )
                                )
                                form_field(
                                    id: &input_id,
                                    label: "New password",
                                    name: "password",
                                    kind: "password",
                                    autocomplete: "new-password",
                                    error: error
                                )
                                dialog_footer(
                                    cancel_button(dialog_id: &dialog_id)
                                    button(attrs: attributes! { type="submit" }, "Set password")
                                )
                            )
                        )
                    )
                }
                for admin in &admins {
                    let dialog_id = delete_id(admin.id);
                    let action = format!("{ADMINS_PATH}/{}/delete", admin.id);
                    alert_dialog(
                        open: false,
                        attrs: attributes! { id=(dialog_id.as_str()) aria-label=(format!("Delete {}", admin.name)) },
                        dialog_content(
                            delete_form(
                                action: &action,
                                yourself: admin.id == me,
                                dialog_header(
                                    if admin.id == me {
                                        dialog_title("Delete yourself?")
                                        dialog_description("You can no longer sign in to the admin app, and you are signed out now.")
                                    } else {
                                        dialog_title("Delete " (admin.name.as_str()) "?")
                                        dialog_description((admin.name.as_str()) " can no longer sign in to the admin app.")
                                    }
                                )
                                dialog_footer(
                                    cancel_button(dialog_id: &dialog_id)
                                    button(
                                        variant: ButtonVariant::Destructive,
                                        attrs: attributes! { type="submit" },
                                        "Delete admin"
                                    )
                                )
                            )
                        )
                    )
                }
            </div>
        )
    })
}

fn set_password_id(admin_id: Uuid) -> String {
    format!("set-password-{admin_id}")
}

fn delete_id(admin_id: Uuid) -> String {
    format!("delete-admin-{admin_id}")
}

#[component]
async fn delete_form(
    action: &str,
    yourself: bool,
    #[default] child: Child<'_>,
) -> topcoat::Result<impl View> {
    Ok(view! {
        if yourself {
            <form method="post" action=(action) class="flex flex-col gap-4">
                csrf_field()
                (child)
            </form>
        } else {
            swap_form(action: action, (child))
        }
    })
}

enum PasswordChange {
    None,
    Changed,
    WrongCurrentPassword,
    EmptyNewPassword,
}

#[page("/admin/account/password")]
async fn account_password(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    Ok(view! { account_password_page(change: PasswordChange::None) })
}

#[derive(Deserialize)]
struct ChangePassword {
    #[serde(default)]
    csrf: String,
    current_password: String,
    new_password: String,
}

#[page(POST "/admin/account/password")]
async fn change_password(cx: &Cx, Form(form): Form<ChangePassword>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    let me = require_admin(cx).await?;
    let token = cookies(cx)
        .get(SESSION_COOKIE)
        .map(|x| x.value().to_owned())
        .unwrap_or_default();
    let change = match application::change_own_admin_password::call(
        app_context::<AppState>(cx),
        me,
        &token,
        form.current_password,
        form.new_password,
    )
    .await
    {
        Ok(()) => {
            log_admin_action(AdminAction::AccountChangePassword, me);
            PasswordChange::Changed
        }
        Err(error) if error.find_source::<WrongCurrentPassword>().is_some() => {
            PasswordChange::WrongCurrentPassword
        }
        Err(error) if error.find_source::<EmptyPassword>().is_some() => {
            PasswordChange::EmptyNewPassword
        }
        Err(error) => return Err(error.into()),
    };
    Ok(view! { account_password_page(change: change) })
}

#[component]
async fn account_password_page(change: PasswordChange) -> topcoat::Result<impl View> {
    Ok(view! {
        admin_page(
            path: ACCOUNT_PASSWORD_PATH,
            title: "Change my password",
            <div id=(ACCOUNT_PASSWORD_ID) hx-target="this" hx-select=(format!("#{ACCOUNT_PASSWORD_ID}")) hx-swap="outerHTML" class="flex max-w-sm flex-col gap-4">
                if let PasswordChange::Changed = change {
                    alert(alert_title("Password changed. You are signed out of the admin app everywhere else."))
                }
                swap_form(
                    action: ACCOUNT_PASSWORD_PATH,
                    field_group(
                        form_field(
                            id: "current-password",
                            label: "Current password",
                            name: "current_password",
                            kind: "password",
                            autocomplete: "current-password",
                            error: matches!(change, PasswordChange::WrongCurrentPassword)
                                .then_some("The current password is wrong.")
                        )
                        form_field(
                            id: "new-password",
                            label: "New password",
                            name: "new_password",
                            kind: "password",
                            autocomplete: "new-password",
                            error: matches!(change, PasswordChange::EmptyNewPassword)
                                .then_some(ENTER_A_PASSWORD)
                        )
                    )
                    <div>
                        button(attrs: attributes! { type="submit" }, "Change password")
                    </div>
                )
            </div>
        )
    })
}

#[component]
async fn swap_form(action: &str, #[default] child: Child<'_>) -> topcoat::Result<impl View> {
    Ok(view! {
        <form method="post" action=(action) hx-post=(action) class="flex flex-col gap-4">
            csrf_field()
            (child)
        </form>
    })
}

#[component]
async fn form_field(
    id: &str,
    label: &str,
    name: &str,
    kind: &str,
    autocomplete: &str,
    #[default] value: &str,
    #[default] error: Option<&str>,
) -> topcoat::Result<impl View> {
    let error_id = format!("{id}-error");
    Ok(view! {
        field(
            field_label(attrs: attributes! { for=(id) }, (label))
            input(attrs: attributes! {
                id=(id)
                name=(name)
                type=(kind)
                autocomplete=(autocomplete)
                value=((!value.is_empty()).then_some(value))
                required=(true)
                aria-invalid=(error.map(|_| "true"))
                aria-describedby=(error.map(|_| error_id.as_str()))
            })
            if let Some(error) = error {
                field_error(attrs: attributes! { id=(error_id.as_str()) }, (error))
            }
        )
    })
}

#[component]
async fn cancel_button(dialog_id: &str) -> topcoat::Result<impl View> {
    Ok(view! {
        button(
            variant: ButtonVariant::Ghost,
            attrs: attributes! { type="button" commandfor=(dialog_id) command="close" },
            "Cancel"
        )
    })
}
