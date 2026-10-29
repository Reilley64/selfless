mod support;

use jiff::SignedDuration;
use jiff::Timestamp;
use reqwest::StatusCode;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::Browser;
use support::Page;
use support::TestDatabase;
use support::TestServer;
use uuid::Uuid;

const RAVI_PASSWORD: &str = "another password";

fn utc(at: Timestamp) -> String {
    at.strftime("%Y-%m-%d %H:%M UTC").to_string()
}

fn rows(page: &Page) -> Vec<Vec<String>> {
    page.body
        .split("<tr")
        .skip(1)
        .map(|row| {
            let row = &row[..row.find("</tr>").unwrap()];
            row.split("<td")
                .skip(1)
                .map(|cell| text(&cell[cell.find('>').unwrap() + 1..cell.find("</td>").unwrap()]))
                .collect::<Vec<_>>()
        })
        .filter(|x| !x.is_empty())
        .collect()
}

fn text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for char in html.chars() {
        match char {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(char),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn create_admin(server: &TestServer, name: &str, password: &str) -> Uuid {
    application::create_admin::call(&server.state, name.to_owned(), password.to_owned())
        .await
        .unwrap()
        .id
}

async fn signed_in(server: &TestServer, name: &str, password: &str) -> Browser {
    let mut browser = server.browser();
    let page = browser.sign_in(name, password).await;
    assert_eq!(page.status, StatusCode::SEE_OTHER, "{name} signs in");
    browser
}

#[tokio::test]
async fn the_admins_page_lists_each_admin_with_their_created_date_and_last_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let maya_created = server.clock.now();
    server.clock.advance(SignedDuration::from_mins(90));
    let ravi_created = server.clock.now();
    create_admin(&server, "Ravi", RAVI_PASSWORD).await;
    server.clock.advance(SignedDuration::from_mins(90));
    let maya_signed_in = server.clock.now();
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
    let rows = rows(&admins);
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
    drop(server);
    database.drop().await;
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
    rows(page).into_iter().map(|x| x[0].clone()).collect()
}

#[tokio::test]
async fn an_admin_creates_another_admin_who_can_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_taken_admin_name_shows_an_error_on_the_name_field() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

    let page = submit(
        &mut browser,
        "/admin/admins",
        &[
            ("name", &ADMIN_NAME.to_uppercase()),
            ("password", RAVI_PASSWORD),
        ],
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn setting_another_admins_password_signs_them_out_and_needs_no_current_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let ravi = create_admin(&server, "Ravi", RAVI_PASSWORD).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn your_own_row_links_to_change_my_password_instead_of_setting_it() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let ravi = create_admin(&server, "Ravi", RAVI_PASSWORD).await;
    let maya = database.admin_id(ADMIN_NAME).await;
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
    let rows = rows(&page);
    assert!(rows[0][3].contains("Change my password"), "{rows:?}");
    assert!(!rows[0][3].contains("Set password"), "{rows:?}");
    assert!(rows[1][3].contains("Set password"), "{rows:?}");
    assert!(page.body.contains(r#"href="/admin/account/password""#));
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn changing_my_password_keeps_this_sign_in_and_ends_my_others() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_current_password_shows_an_error_on_the_field_and_changes_nothing() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_another_admin_removes_them_and_signs_them_out() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let ravi = create_admin(&server, "Ravi", RAVI_PASSWORD).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_yourself_signs_you_out_and_shows_the_sign_in_form() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    create_admin(&server, "Ravi", RAVI_PASSWORD).await;
    let maya = database.admin_id(ADMIN_NAME).await;
    let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

    let deleted = submit(&mut browser, &format!("/admin/admins/{maya}/delete"), &[]).await;
    let after = browser.get("/admin/admins").await;

    assert_eq!(deleted.status, StatusCode::SEE_OTHER);
    assert_eq!(deleted.location.as_deref(), Some("/admin/sign-in"));
    assert_eq!(browser.cookie("session"), None);
    assert_eq!(after.location.as_deref(), Some("/admin/sign-in"));
    let mut ravi_browser = signed_in(&server, "Ravi", RAVI_PASSWORD).await;
    assert_eq!(names(&admins_page(&mut ravi_browser).await), ["Ravi"]);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_the_last_admin_shows_an_inline_error() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let maya = database.admin_id(ADMIN_NAME).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn each_sign_in_sets_the_last_sign_in_time() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
    server.clock.advance(SignedDuration::from_hours(3));
    let latest = server.clock.now();

    let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;
    server.clock.advance(SignedDuration::from_mins(5));
    server.browser().sign_in(ADMIN_NAME, "wrong").await;

    let page = admins_page(&mut browser).await;
    assert_eq!(rows(&page)[0][2], utc(latest));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_forms_without_the_csrf_token_are_rejected() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let ravi = create_admin(&server, "Ravi", RAVI_PASSWORD).await;
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
    drop(server);
    database.drop().await;
}
