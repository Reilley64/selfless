use crate::atoms::csrf_field::csrf_token;
use topcoat::asset::Asset;
use topcoat::asset::asset;
use topcoat::context::Cx;
use topcoat::view::Child;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;

const STYLES: Asset = asset!("../../dist/styles.css");
const HTMX: Asset = asset!(
    "https://cdn.jsdelivr.net/npm/htmx.org@2.0.11/dist/htmx.min.js",
    checksum: "sha256:d6fdc75f204e6bdefa99b69bf1e6d4ac69b8a364f77929f45c13476b4000f717"
);
const LOCAL_TIME_SCRIPT: Asset = asset!("../../assets/time.js");
const HTMX_CONFIG: &str =
    r#"{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false}"#;

#[component]
pub(crate) async fn document(cx: &Cx, #[default] child: Child<'_>) -> topcoat::Result<impl View> {
    let htmx_headers = format!(r#"{{"X-CSRF-Token":"{}"}}"#, csrf_token(cx));
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="htmx-config" content=(HTMX_CONFIG) />
                <title>"selfless admin"</title>
                <link rel="stylesheet" href=(STYLES) />
                <script src=(HTMX) defer=(true)></script>
                <script src=(LOCAL_TIME_SCRIPT) defer=(true)></script>
            </head>
            <body class="bg-background font-sans text-foreground antialiased" hx-headers=(htmx_headers)>
                (child)
            </body>
        </html>
    })
}

#[cfg(test)]
mod tests {
    use crate::test::ADMIN_NAME;
    use crate::test::ADMIN_PASSWORD;
    use crate::test::create_admin;
    use crate::test::serve;
    use http::StatusCode;
    use test_app::TestApp;

    #[tokio::test]
    async fn admin_pages_send_a_strict_content_security_policy_and_load_every_script_from_a_file() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = server.browser();
        let sign_in = browser.get("/admin/sign-in").await;
        browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
        let sync = browser.get("/admin/sync").await;

        for page in [&sign_in, &sync] {
            let policy = page
                .header("content-security-policy")
                .expect("a Content-Security-Policy");
            assert!(policy.contains("default-src 'self'"), "{policy}");
            assert!(policy.contains("script-src 'self'"), "{policy}");
            assert!(policy.contains("object-src 'none'"), "{policy}");
            assert!(!policy.contains("'unsafe-inline'"), "{policy}");
            assert!(!policy.contains("'unsafe-eval'"), "{policy}");
            assert!(page.body.contains("<script"), "{}", page.body);
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
    }
}
