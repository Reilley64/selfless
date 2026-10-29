mod support;

use jiff::SignedDuration;
use jiff::Timestamp;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::json;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::Browser;
use support::Page;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;

const EMAIL: &str = "ravi@household.example";
const NEW_PASSWORD: &str = "a new passphrase about coffee";
const WEEK: SignedDuration = SignedDuration::from_hours(7 * 24);

async fn admin(server: &TestServer) -> Browser {
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    browser
}

async fn submit(browser: &mut Browser, page: &Page, button: &str, fields: &[(&str, &str)]) -> Page {
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

fn token_from_last_email(server: &TestServer) -> String {
    let body = server.mailer.sent().last().unwrap().body.clone();
    let link = format!("{}/invite#", server.base_url());
    let start = body
        .find(&link)
        .unwrap_or_else(|| panic!("the email has no invite link on the admin's origin\n{body}"))
        + link.len();
    body[start..].split_whitespace().next().unwrap().to_owned()
}

async fn accept(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(
            Method::POST,
            "/api/invites/accept/v1",
            None,
            Some(json!({ "token": token, "name": "Ravi", "password": PASSWORD })),
        )
        .await
        .0
}

async fn me(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(Method::GET, "/api/me/v1", Some(token), None)
        .await
        .0
}

async fn sign_in(server: &TestServer, password: &str, client_kind: &str) -> String {
    let (status, body) = server.sign_in(NAME, password, client_kind).await;
    assert_eq!(status, StatusCode::CREATED);
    body["token"].as_str().unwrap().to_owned()
}

async fn sign_in_on(server: &TestServer, client_kind: &str, device_name: &str) -> String {
    let (status, body) = server
        .request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": NAME,
                "password": PASSWORD,
                "clientKind": client_kind,
                "deviceName": device_name,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    body["token"].as_str().unwrap().to_owned()
}

async fn confirm(browser: &mut Browser, page: &Page, dialog_title: &str) -> Page {
    let title_at = page
        .body
        .find(&format!(">{dialog_title}<"))
        .unwrap_or_else(|| panic!("the page has no {dialog_title:?} dialog\n{}", page.body));
    let start = title_at + page.body[title_at..].find(r#"action=""#).unwrap() + r#"action=""#.len();
    let length = page.body[start..].find('"').unwrap();
    let action = page.body[start..start + length].to_owned();
    browser
        .post_form(&action, &[("csrf", &page.csrf_token())])
        .await
}

#[tokio::test]
async fn users_lists_each_user_with_their_created_time_signed_in_devices_and_last_use() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let created_at = server.clock.now();
    server.create_user(NAME, PASSWORD).await;
    sign_in(&server, PASSWORD, "web").await;
    let tv = sign_in(&server, PASSWORD, "appleTv").await;
    server.clock.advance(SignedDuration::from_hours(31 * 24));
    let used_at = server.clock.now();
    me(&server, &tv).await;
    server.create_user("Ravi", PASSWORD).await;
    let mut browser = admin(&server).await;

    let users = browser.get("/admin/users").await;

    assert_eq!(users.status, StatusCode::OK);
    assert_eq!(users.link("Users"), "/admin/users");
    assert_eq!(
        rows_with(&users, NAME),
        [vec![
            String::new(),
            NAME.to_owned(),
            utc(created_at),
            "1".to_owned(),
            utc(used_at),
        ]]
    );
    assert_eq!(
        rows_with(&users, "Ravi"),
        [vec![
            String::new(),
            "Ravi".to_owned(),
            utc(used_at),
            "0".to_owned(),
            "Never".to_owned(),
        ]]
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn inviting_an_address_emails_a_link_on_the_admins_origin_and_lists_the_pending_invite() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = admin(&server).await;
    let sent_at = server.clock.now();

    let invited = invite(&mut browser, EMAIL).await;

    assert_eq!(invited.status, StatusCode::OK);
    assert!(
        invited.text().contains(&format!("Invite sent to {EMAIL}.")),
        "{}",
        invited.body
    );
    let sent = server.mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].recipient, EMAIL);
    assert_eq!(
        accept(&server, &token_from_last_email(&server)).await,
        StatusCode::CREATED
    );
    let invites = rows_with(&invited, EMAIL);
    assert_eq!(invites.len(), 1);
    assert_eq!(
        invites[0][..3],
        [EMAIL.to_owned(), utc(sent_at), utc(sent_at + WEEK)]
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_address_that_already_has_a_user_or_a_pending_invite_shows_an_error_in_any_capitals() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let mut browser = admin(&server).await;
    invite(&mut browser, EMAIL).await;

    let user = invite(&mut browser, "NOOR@Example.COM").await;
    let pending = invite(&mut browser, &EMAIL.to_uppercase()).await;

    for page in [user, pending] {
        assert_eq!(page.status, StatusCode::OK);
        assert!(
            page.text()
                .contains("A user or a pending invite already has this address."),
            "{}",
            page.body
        );
    }
    assert_eq!(server.mailer.sent().len(), 1);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn resending_an_invite_emails_a_new_link_and_the_old_link_stops_working() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = admin(&server).await;
    invite(&mut browser, EMAIL).await;
    let old = token_from_last_email(&server);
    server.clock.advance(SignedDuration::from_hours(24));
    let resent_at = server.clock.now();
    let mut browser = admin(&server).await;
    let users = browser.get("/admin/users").await;

    let resent = submit(&mut browser, &users, "Resend", &[]).await;

    assert!(
        resent
            .text()
            .contains(&format!("Invite sent again to {EMAIL}.")),
        "{}",
        resent.body
    );
    assert_eq!(
        rows_with(&resent, EMAIL)[0][..3],
        [EMAIL.to_owned(), utc(resent_at), utc(resent_at + WEEK)]
    );
    assert_eq!(server.mailer.sent().len(), 2);
    let new = token_from_last_email(&server);
    assert_eq!(accept(&server, &old).await, StatusCode::NOT_FOUND);
    assert_eq!(accept(&server, &new).await, StatusCode::CREATED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn cancelling_an_invite_removes_it_and_its_link_stops_working() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = admin(&server).await;
    invite(&mut browser, EMAIL).await;
    let token = token_from_last_email(&server);
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
    assert_eq!(accept(&server, &token).await, StatusCode::NOT_FOUND);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_invite_shows_as_expired_7_days_after_it_was_sent() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = admin(&server).await;
    invite(&mut browser, EMAIL).await;

    server.clock.advance(WEEK - SignedDuration::from_secs(1));
    let before = admin(&server).await.get("/admin/users").await;
    server.clock.advance(SignedDuration::from_secs(1));
    let after = admin(&server).await.get("/admin/users").await;

    assert!(!rows_with(&before, EMAIL)[0].concat().contains("Expired"));
    assert!(rows_with(&after, EMAIL)[0].concat().contains("Expired"));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_users_row_opens_their_page_with_their_name_and_actions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn resetting_a_password_signs_the_user_out_everywhere() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let web = sign_in(&server, PASSWORD, "web").await;
    let tv = sign_in(&server, PASSWORD, "appleTv").await;
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
    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::UNAUTHORIZED);
    let (old, _) = server.sign_in(NAME, PASSWORD, "web").await;
    assert_eq!(old, StatusCode::UNAUTHORIZED);
    sign_in(&server, NEW_PASSWORD, "web").await;
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_user_is_deleted_only_when_the_typed_name_matches_in_any_capitals() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let token = sign_in(&server, PASSWORD, "web").await;
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
    assert_eq!(me(&server, &token).await, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn renaming_a_user_keeps_them_signed_in_and_their_apps_get_the_new_name() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let web = sign_in(&server, PASSWORD, "web").await;
    let tv = sign_in(&server, PASSWORD, "appleTv").await;
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
        let (status, body) = server
            .request(Method::GET, "/api/me/v1", Some(token), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "Nour");
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_user_can_be_renamed_to_their_name_in_other_capitals() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let mut browser = admin(&server).await;
    let user = user_page(&mut browser, NAME).await;

    submit(&mut browser, &user, "Save", &[("name", "NOOR")]).await;

    let users = browser.get("/admin/users").await;
    assert_eq!(rows_with(&users, "NOOR").len(), 1);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_rename_to_a_taken_or_empty_name_shows_an_error_and_keeps_the_name() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    server.create_user("Ravi", PASSWORD).await;
    let mut browser = admin(&server).await;
    let user = user_page(&mut browser, "Ravi").await;

    let taken = submit(&mut browser, &user, "Save", &[("name", "nOOR")]).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_users_page_lists_the_devices_they_are_signed_in_on() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    sign_in_on(&server, "web", "Firefox on macOS").await;
    server.clock.advance(SignedDuration::from_hours(31 * 24));
    let tv_signed_in_at = server.clock.now();
    let tv = sign_in_on(&server, "appleTv", "Living Room").await;
    server.clock.advance(SignedDuration::from_hours(2));
    let google_tv_signed_in_at = server.clock.now();
    sign_in_on(&server, "googleTv", "Bedroom").await;
    server.clock.advance(SignedDuration::from_hours(3));
    let tv_used_at = server.clock.now();
    me(&server, &tv).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_out_one_device_ends_only_that_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let living_room = sign_in_on(&server, "appleTv", "Living Room").await;
    let bedroom = sign_in_on(&server, "googleTv", "Bedroom").await;
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
    assert_eq!(me(&server, &living_room).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &bedroom).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_out_everywhere_ends_every_sign_in_of_that_user_only() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    server.create_user("Ravi", PASSWORD).await;
    let web = sign_in_on(&server, "web", "Firefox on macOS").await;
    let tv = sign_in_on(&server, "appleTv", "Living Room").await;
    let (_, ravi) = server.sign_in("Ravi", PASSWORD, "appleTv").await;
    let mut browser = admin(&server).await;
    let user = user_page(&mut browser, NAME).await;

    let signed_out = confirm(&mut browser, &user, &format!("Sign {NAME} out everywhere?")).await;

    assert_eq!(signed_out.status, StatusCode::OK);
    assert!(
        signed_out
            .text()
            .contains(&format!("Signed {NAME} out everywhere.")),
        "{}",
        signed_out.body
    );
    assert_eq!(signed_out.table_rows(), Vec::<Vec<String>>::new());
    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me(&server, ravi["token"].as_str().unwrap()).await,
        StatusCode::OK
    );
    drop(server);
    database.drop().await;
}
