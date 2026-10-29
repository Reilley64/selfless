mod support;

use jiff::SignedDuration;
use reqwest::StatusCode;
use settings::AdminSettings;
use sha2::Digest;
use sha2::Sha256;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::FakeClock;
use support::FakeMailer;
use support::TestDatabase;
use support::TestServer;
use support::settings;
use tokio::time::Instant;

#[tokio::test]
async fn the_first_admin_signs_in_and_sees_the_admin_app() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let signed_in = browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    let home = browser.get("/admin").await;
    let sync = browser.get("/admin/sync").await;

    assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_in.location.as_deref(), Some("/admin"));
    assert_eq!(home.status, StatusCode::SEE_OTHER);
    assert_eq!(home.location.as_deref(), Some("/admin/sync"));
    assert_eq!(sync.status, StatusCode::OK);
    assert!(sync.body.contains(">Sync status<"), "{}", sync.body);
}

#[tokio::test]
async fn every_admin_page_shows_the_server_name_and_version() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    let sync = browser.get("/admin/sync").await;

    assert!(sync.body.contains(">selfless<"), "{}", sync.body);
    assert!(
        sync.body
            .contains(&format!(">{}<", env!("CARGO_PKG_VERSION"))),
        "{}",
        sync.body
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_pages_redirect_to_sign_in_without_an_admin_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let without_cookie = browser.get("/admin/sync").await;
    browser.set_cookie("session", "not-a-session");
    let with_unknown_cookie = browser.get("/admin/sync").await;

    for page in [without_cookie, with_unknown_cookie] {
        assert_eq!(page.status, StatusCode::SEE_OTHER);
        assert_eq!(page.location.as_deref(), Some("/admin/sign-in"));
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_out_from_the_account_menu_ends_the_admin_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    let token = browser.cookie("session").unwrap().to_owned();

    let sync = browser.get("/admin/sync").await;
    let signed_out = browser
        .post_form(
            &sync.form_action("Sign out"),
            &[("csrf", &sync.csrf_token())],
        )
        .await;
    let home = browser.get("/admin/sync").await;
    browser.set_cookie("session", &token);
    let home_with_old_token = browser.get("/admin/sync").await;

    assert_eq!(signed_out.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_out.location.as_deref(), Some("/admin/sign-in"));
    for page in [home, home_with_old_token] {
        assert_eq!(page.status, StatusCode::SEE_OTHER);
        assert_eq!(page.location.as_deref(), Some("/admin/sign-in"));
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_admin_session_cookie_is_http_only_secure_strict_and_scoped_to_admin() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let signed_in = browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    let session = signed_in
        .set_cookies
        .iter()
        .find(|x| x.starts_with("session="))
        .expect("a session cookie");
    let attributes = session.split("; ").skip(1).collect::<Vec<_>>();
    for attribute in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/admin"] {
        assert!(attributes.contains(&attribute), "{session}");
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_database_stores_only_the_sha_256_hash_of_the_admin_session_token() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    let token = browser.cookie("session").unwrap();
    let rows = database
        .query("SELECT token_hash FROM admin_sessions")
        .await;
    let stored: Vec<u8> = rows[0].get("token_hash");
    assert_eq!(rows.len(), 1);
    assert_eq!(stored, Sha256::digest(token.as_bytes()).to_vec());
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_forms_without_the_csrf_token_are_rejected() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.get("/admin/sign-in").await;

    let sign_in_without_token = browser
        .post_form(
            "/admin/sign-in",
            &[("name", ADMIN_NAME), ("password", ADMIN_PASSWORD)],
        )
        .await;
    let sign_in_with_wrong_token = browser
        .post_form(
            "/admin/sign-in",
            &[
                ("csrf", "wrong"),
                ("name", ADMIN_NAME),
                ("password", ADMIN_PASSWORD),
            ],
        )
        .await;
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_name_and_a_wrong_password_return_the_same_error() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let wrong_name = browser.sign_in("Noor", ADMIN_PASSWORD).await;
    let wrong_password = browser.sign_in(ADMIN_NAME, "wrong").await;

    assert_eq!(wrong_name.status, StatusCode::OK);
    assert!(wrong_name.body.contains("Wrong name or password."));
    assert_eq!(wrong_name.status, wrong_password.status);
    assert_eq!(wrong_name.text(), wrong_password.text());
    assert_eq!(browser.cookie("session"), None);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let signed_in = browser
        .sign_in(&ADMIN_NAME.to_uppercase(), ADMIN_PASSWORD)
        .await;

    assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_in.location.as_deref(), Some("/admin"));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_admin_session_ends_after_12_hours_without_use() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    server.clock.advance(SignedDuration::from_hours(11));
    let after_11_hours = browser.get("/admin/sync").await;
    server.clock.advance(SignedDuration::from_hours(11));
    let after_11_more_hours = browser.get("/admin/sync").await;
    server.clock.advance(SignedDuration::from_hours(12));
    let after_12_idle_hours = browser.get("/admin/sync").await;

    assert_eq!(after_11_hours.status, StatusCode::OK);
    assert_eq!(after_11_more_hours.status, StatusCode::OK);
    assert_eq!(after_12_idle_hours.status, StatusCode::SEE_OTHER);
    assert_eq!(
        after_12_idle_hours.location.as_deref(),
        Some("/admin/sign-in")
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_server_refuses_to_start_without_a_first_admin() {
    let database = TestDatabase::create().await;

    for admin in [
        AdminSettings {
            username: None,
            password: None,
        },
        AdminSettings {
            username: Some(ADMIN_NAME.to_owned()),
            password: None,
        },
        AdminSettings {
            username: None,
            password: Some(ADMIN_PASSWORD.to_owned()),
        },
    ] {
        let mut settings = settings(database.url());
        settings.admin = admin;

        let error = selfless::start(
            settings,
            FakeClock::new().port(),
            FakeMailer::default().send_email(),
        )
        .await
        .err()
        .expect("the server refuses to start");

        let message = error.to_string();
        assert!(message.contains("ADMIN__USERNAME"), "{message}");
        assert!(message.contains("ADMIN__PASSWORD"), "{message}");
    }
    database.drop().await;
}

#[tokio::test]
async fn the_first_admin_variables_are_ignored_once_an_admin_exists() {
    let database = TestDatabase::create().await;
    drop(TestServer::start(database.url()).await);
    let mut settings = settings(database.url());
    settings.admin = AdminSettings {
        username: Some("Noor".to_owned()),
        password: Some("another password".to_owned()),
    };
    let server = TestServer::start_with(settings, FakeClock::new()).await;
    let mut browser = server.browser();

    let new_admin = browser.sign_in("Noor", "another password").await;
    let first_admin = browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    assert_eq!(new_admin.status, StatusCode::OK);
    assert!(new_admin.body.contains("Wrong name or password."));
    assert_eq!(first_admin.status, StatusCode::SEE_OTHER);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_unknown_name_takes_as_long_as_a_wrong_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    let csrf = browser.get("/admin/sign-in").await.csrf_token();
    let mut sign_in_time = async |name: &str| {
        let start = Instant::now();
        browser
            .post_form(
                "/admin/sign-in",
                &[("csrf", &csrf), ("name", name), ("password", "wrong")],
            )
            .await;
        start.elapsed()
    };

    let wrong_password = sign_in_time(ADMIN_NAME).await;
    let unknown_name = sign_in_time("Noor").await;

    assert!(
        unknown_name * 2 > wrong_password,
        "unknown name {unknown_name:?}, wrong password {wrong_password:?}"
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn htmx_requests_without_the_csrf_token_are_rejected() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
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
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_pages_send_a_strict_content_security_policy_and_need_no_inline_script() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    let sign_in = browser.get("/admin/sign-in").await;
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    let home = browser.get("/admin").await;
    let sync = browser.get("/admin/sync").await;

    for page in [&sign_in, &home, &sync] {
        let policy = page
            .header("content-security-policy")
            .expect("a Content-Security-Policy");
        assert!(policy.contains("script-src 'self'"), "{policy}");
        assert!(policy.contains("object-src 'none'"), "{policy}");
        assert!(!policy.contains("'unsafe-inline'"), "{policy}");
        assert!(!policy.contains("'unsafe-eval'"), "{policy}");
    }
    for page in [&sign_in, &sync] {
        for script in page.body.split("<script").skip(1) {
            let tag = &script[..script.find('>').unwrap()];
            let start = tag
                .find(r#" src=""#)
                .unwrap_or_else(|| panic!("inline script: <script{tag}>"))
                + r#" src=""#.len();
            let src = &tag[start..start + tag[start..].find('"').unwrap()];
            assert_eq!(browser.get(src).await.status, StatusCode::OK, "{src}");
        }
    }
    drop(server);
    database.drop().await;
}
