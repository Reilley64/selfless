use crate::ACCOUNT_PASSWORD_PATH;
use crate::ADMINS_PATH;
use crate::AdminAction;
use crate::SESSION_COOKIE;
use crate::SIGN_IN_PATH;
use crate::admin_cookie;
use crate::atoms::csrf_field::csrf_field;
use crate::atoms::local_time::TimeFormat;
use crate::atoms::local_time::local_time;
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
use crate::log_admin_action;
use crate::require_admin;
use crate::templates::admin_page::admin_page;
use crate::verify_csrf;
use application::admins::change_own_admin_password::WrongCurrentPassword;
use application::admins::create_admin::NameTaken;
use domain::LastAdmin;
use domain::Password;
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
    let outcome = match Password::new(form.password) {
        Ok(password) => match application::admins::create_admin::call(
            app_context::<AppState>(cx),
            form.name.clone(),
            password,
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
            Err(error) => return Err(error.into()),
        },
        Err(_) => Outcome::NewAdminInvalid {
            name: form.name,
            name_error: None,
            password_error: Some(ENTER_A_PASSWORD),
        },
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
    let outcome = match Password::new(form.password) {
        Ok(password) => {
            application::admins::change_admin_password::call(
                app_context::<AppState>(cx),
                admin_id,
                password,
            )
            .await?;
            log_admin_action(AdminAction::AdminSetPassword, admin_id);
            Outcome::Done("Password set.")
        }
        Err(_) => Outcome::SetPasswordInvalid {
            admin_id,
            error: ENTER_A_PASSWORD,
        },
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
    let outcome = match application::admins::delete_admin::call(
        app_context::<AppState>(cx),
        admin_id,
    )
    .await
    {
        Ok(_) => {
            log_admin_action(AdminAction::AdminDelete, admin_id);
            if admin_id == me {
                cookies(cx).remove(admin_cookie(cx, SESSION_COOKIE, String::new()));
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
    let admins = application::admins::list_admins::call(app_context::<AppState>(cx))
        .await?
        .admins;
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
    let change = match Password::new(form.new_password) {
        Ok(new_password) => match application::admins::change_own_admin_password::call(
            app_context::<AppState>(cx),
            me,
            &token,
            form.current_password,
            new_password,
        )
        .await
        {
            Ok(_) => {
                log_admin_action(AdminAction::AccountChangePassword, me);
                PasswordChange::Changed
            }
            Err(error) if error.find_source::<WrongCurrentPassword>().is_some() => {
                PasswordChange::WrongCurrentPassword
            }
            Err(error) => return Err(error.into()),
        },
        Err(_) => PasswordChange::EmptyNewPassword,
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

#[cfg(test)]
mod tests {
    use crate::test::ADMIN_NAME;
    use crate::test::ADMIN_PASSWORD;
    use crate::test::Logs;
    use crate::test::create_admin;
    use crate::test::serve;
    use crate::test::signed_in;
    use http::StatusCode;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use test_app::TestApp;
    use test_fixtures::Browser;
    use test_fixtures::Page;

    const RAVI_PASSWORD: &str = "another password";

    fn utc(at: Timestamp) -> String {
        at.strftime("%Y-%m-%d %H:%M UTC").to_string()
    }

    #[tokio::test]
    async fn the_admins_page_lists_each_admin_with_their_created_date_and_last_sign_in() {
        let app = TestApp::new().await;
        let maya_created = app.clock.now();
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        app.clock.advance(SignedDuration::from_mins(90));
        let ravi_created = app.clock.now();
        create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        app.clock.advance(SignedDuration::from_mins(90));
        let maya_signed_in = app.clock.now();
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let sync = browser.get("/admin/sync").await;
        let admins = browser.get("/admin/admins").await;

        assert!(
            sync.body.contains(r#"href="/admin/admins""#),
            "{}",
            sync.body
        );
        assert!(sync.body.contains(">Admins<"), "{}", sync.body);
        assert_eq!(admins.status, StatusCode::OK);
        assert!(admins.body.contains(">New admin<"), "{}", admins.body);
        let rows = admins.table_rows();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(
            rows[0][..3],
            [
                ADMIN_NAME.to_owned(),
                utc(maya_created),
                utc(maya_signed_in)
            ]
        );
        assert_eq!(
            rows[1][..3],
            ["Ravi".to_owned(), utc(ravi_created), "Never".to_owned()]
        );
    }

    fn has_form(page: &Page, action: &str) -> bool {
        page.body.contains(&format!(r#"action="{action}""#))
    }

    async fn admins_page(browser: &mut Browser) -> Page {
        let page = browser.get("/admin/admins").await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        page
    }

    async fn submit(browser: &mut Browser, action: &str, fields: &[(&str, &str)]) -> Page {
        let page = admins_page(browser).await;
        assert!(
            has_form(&page, action),
            "no form posts to {action}\n{}",
            page.body
        );
        let csrf = page.csrf_token();
        let mut fields = fields.to_vec();
        fields.push(("csrf", &csrf));
        browser.post_form(action, &fields).await
    }

    fn names(page: &Page) -> Vec<String> {
        page.table_rows()
            .into_iter()
            .map(|x| x[0].clone())
            .collect()
    }

    #[tokio::test]
    async fn an_admin_creates_another_admin_who_can_sign_in() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let created = submit(
            &mut browser,
            "/admin/admins",
            &[("name", "Ravi"), ("password", RAVI_PASSWORD)],
        )
        .await;

        assert_eq!(created.status, StatusCode::OK);
        assert!(
            created.text().contains("Admin created."),
            "{}",
            created.body
        );
        assert_eq!(names(&created), [ADMIN_NAME, "Ravi"]);
        signed_in(&server, "Ravi", RAVI_PASSWORD).await;
    }

    #[tokio::test]
    async fn a_taken_admin_name_shows_an_error_on_the_name_field() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let page = submit(
            &mut browser,
            "/admin/admins",
            &[("name", ADMIN_NAME), ("password", RAVI_PASSWORD)],
        )
        .await;

        assert_eq!(page.status, StatusCode::OK);
        assert!(
            page.text().contains("An admin already has this name."),
            "{}",
            page.body
        );
        assert!(
            page.body.contains(r#"aria-invalid="true""#),
            "{}",
            page.body
        );
        assert_eq!(names(&page), [ADMIN_NAME]);
    }

    #[tokio::test]
    async fn setting_another_admins_password_signs_them_out_and_needs_no_current_password() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let ravi = create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        let mut ravi_browser = signed_in(&server, "Ravi", RAVI_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let page = submit(
            &mut browser,
            &format!("/admin/admins/{ravi}/password"),
            &[("password", "a new password")],
        )
        .await;

        assert_eq!(page.status, StatusCode::OK);
        assert!(page.text().contains("Password set."), "{}", page.body);
        assert_eq!(
            ravi_browser.get("/admin/admins").await.status,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            server.browser().sign_in("Ravi", RAVI_PASSWORD).await.status,
            StatusCode::OK
        );
        signed_in(&server, "Ravi", "a new password").await;
        admins_page(&mut browser).await;
    }

    #[tokio::test]
    async fn your_own_row_links_to_change_my_password_instead_of_setting_it() {
        let app = TestApp::new().await;
        let maya = create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let ravi = create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let page = admins_page(&mut browser).await;
        let refused = browser
            .post_form(
                &format!("/admin/admins/{maya}/password"),
                &[("csrf", &page.csrf_token()), ("password", "a new password")],
            )
            .await;

        assert!(has_form(&page, &format!("/admin/admins/{ravi}/password")));
        assert!(!has_form(&page, &format!("/admin/admins/{maya}/password")));
        let rows = page.table_rows();
        assert!(rows[0][3].contains("Change my password"), "{rows:?}");
        assert!(!rows[0][3].contains("Set password"), "{rows:?}");
        assert!(rows[1][3].contains("Set password"), "{rows:?}");
        assert!(page.body.contains(r#"href="/admin/account/password""#));
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
    }

    #[tokio::test]
    async fn changing_my_password_keeps_this_sign_in_and_ends_my_others() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut other = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let sync = browser.get("/admin/sync").await;
        let form = browser.get("/admin/account/password").await;
        let changed = browser
            .post_form(
                "/admin/account/password",
                &[
                    ("csrf", &form.csrf_token()),
                    ("current_password", ADMIN_PASSWORD),
                    ("new_password", "a new password"),
                ],
            )
            .await;

        assert!(sync.body.contains(r#"href="/admin/account/password""#));
        assert!(sync.body.contains(">Change my password<"), "{}", sync.body);
        assert_eq!(form.status, StatusCode::OK);
        assert!(has_form(&form, "/admin/account/password"), "{}", form.body);
        assert_eq!(changed.status, StatusCode::OK);
        assert!(
            changed.text().contains("Password changed."),
            "{}",
            changed.body
        );
        assert_eq!(browser.get("/admin/sync").await.status, StatusCode::OK);
        assert_eq!(other.get("/admin/sync").await.status, StatusCode::SEE_OTHER);
        assert_eq!(
            server
                .browser()
                .sign_in(ADMIN_NAME, ADMIN_PASSWORD)
                .await
                .status,
            StatusCode::OK
        );
        signed_in(&server, ADMIN_NAME, "a new password").await;
    }

    #[tokio::test]
    async fn a_wrong_current_password_shows_an_error_on_the_field_and_changes_nothing() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut other = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let form = browser.get("/admin/account/password").await;
        let page = browser
            .post_form(
                "/admin/account/password",
                &[
                    ("csrf", &form.csrf_token()),
                    ("current_password", "wrong"),
                    ("new_password", "a new password"),
                ],
            )
            .await;

        assert_eq!(page.status, StatusCode::OK);
        assert!(
            page.text().contains("The current password is wrong."),
            "{}",
            page.body
        );
        assert!(
            page.body.contains(r#"aria-invalid="true""#),
            "{}",
            page.body
        );
        assert!(!page.text().contains("Password changed."));
        assert_eq!(other.get("/admin/sync").await.status, StatusCode::OK);
        signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
    }

    #[tokio::test]
    async fn deleting_another_admin_removes_them_and_signs_them_out() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let ravi = create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        let mut ravi_browser = signed_in(&server, "Ravi", RAVI_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let page = submit(&mut browser, &format!("/admin/admins/{ravi}/delete"), &[]).await;

        assert_eq!(page.status, StatusCode::OK);
        assert!(page.text().contains("Admin deleted."), "{}", page.body);
        assert_eq!(names(&page), [ADMIN_NAME]);
        assert_eq!(
            ravi_browser.get("/admin/admins").await.status,
            StatusCode::SEE_OTHER
        );
    }

    #[tokio::test]
    async fn deleting_yourself_signs_you_out_and_shows_the_sign_in_form() {
        let app = TestApp::new().await;
        let maya = create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let deleted = submit(&mut browser, &format!("/admin/admins/{maya}/delete"), &[]).await;
        let after = browser.get("/admin/admins").await;

        assert_eq!(deleted.status, StatusCode::SEE_OTHER);
        assert_eq!(deleted.location.as_deref(), Some("/admin/sign-in"));
        assert_eq!(browser.cookie("session"), None);
        assert_eq!(after.location.as_deref(), Some("/admin/sign-in"));
        let mut ravi_browser = signed_in(&server, "Ravi", RAVI_PASSWORD).await;
        assert_eq!(names(&admins_page(&mut ravi_browser).await), ["Ravi"]);
    }

    #[tokio::test]
    async fn deleting_the_last_admin_shows_an_inline_error() {
        let app = TestApp::new().await;
        let maya = create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        let page = submit(&mut browser, &format!("/admin/admins/{maya}/delete"), &[]).await;

        assert_eq!(page.status, StatusCode::OK);
        assert!(
            page.text().contains("The last admin can't be deleted."),
            "{}",
            page.body
        );
        assert_eq!(names(&page), [ADMIN_NAME]);
        admins_page(&mut browser).await;
    }

    #[tokio::test]
    async fn each_sign_in_sets_the_last_sign_in_time() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
        app.clock.advance(SignedDuration::from_hours(3));
        let latest = app.clock.now();

        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
        app.clock.advance(SignedDuration::from_mins(5));
        server.browser().sign_in(ADMIN_NAME, "wrong").await;

        let page = admins_page(&mut browser).await;
        assert_eq!(page.table_rows()[0][2], utc(latest));
    }

    #[tokio::test]
    async fn admin_forms_without_the_csrf_token_are_rejected() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let ravi = create_admin(&app, "Ravi", RAVI_PASSWORD).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        for (action, fields) in [
            (
                "/admin/admins".to_owned(),
                vec![("name", "Noor"), ("password", "password")],
            ),
            (
                format!("/admin/admins/{ravi}/password"),
                vec![("password", "password")],
            ),
            (format!("/admin/admins/{ravi}/delete"), vec![]),
            (
                "/admin/account/password".to_owned(),
                vec![
                    ("current_password", ADMIN_PASSWORD),
                    ("new_password", "password"),
                ],
            ),
        ] {
            assert_eq!(
                browser.post_form(&action, &fields).await.status,
                StatusCode::FORBIDDEN,
                "{action}"
            );
        }
        assert_eq!(
            names(&admins_page(&mut browser).await),
            [ADMIN_NAME, "Ravi"]
        );
        signed_in(&server, "Ravi", RAVI_PASSWORD).await;
    }

    #[tokio::test]
    async fn admin_and_account_actions_log_the_action_and_target_without_names_or_passwords() {
        let app = TestApp::new().await;
        let maya = create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let logs = Logs::capture();
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
        let csrf = browser.get("/admin/admins").await.csrf_token();

        let created = browser
            .post_form(
                "/admin/admins",
                &[
                    ("csrf", &csrf),
                    ("name", "Ravi"),
                    ("password", "ravi's password"),
                ],
            )
            .await;
        assert_eq!(created.status, StatusCode::OK);
        let ravi = application::admins::list_admins::call(&app.state)
            .await
            .unwrap()
            .admins
            .into_iter()
            .find(|x| x.name == "Ravi")
            .unwrap()
            .id
            .to_string();
        browser
            .post_form(
                &format!("/admin/admins/{ravi}/password"),
                &[("csrf", &csrf), ("password", "ravi's new password")],
            )
            .await;
        browser
            .post_form(&format!("/admin/admins/{ravi}/delete"), &[("csrf", &csrf)])
            .await;
        browser
            .post_form(
                "/admin/account/password",
                &[
                    ("csrf", &csrf),
                    ("current_password", ADMIN_PASSWORD),
                    ("new_password", "maya's new password"),
                ],
            )
            .await;

        assert_eq!(
            logs.admin_actions(),
            [
                ("admin.create", ravi.clone()),
                ("admin.set_password", ravi.clone()),
                ("admin.delete", ravi),
                ("account.change_password", maya.to_string()),
            ]
            .map(|(action, target)| (action.to_owned(), target))
        );
        logs.assert_none_contains(&[
            "Ravi",
            ADMIN_NAME,
            ADMIN_PASSWORD,
            "ravi's password",
            "ravi's new password",
            "maya's new password",
        ]);
    }
}
