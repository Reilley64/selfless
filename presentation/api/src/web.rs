use std::sync::LazyLock;
use topcoat::context::Cx;
use topcoat::router::Body;
use topcoat::router::HeaderValue;
use topcoat::router::Layer;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::Path;
use topcoat::router::error::rewrite;
use topcoat::router::header::CONTENT_SECURITY_POLICY;
use topcoat::router::response::Response;
use topcoat::router::response::response_headers;
use topcoat::router::route;

pub(crate) const FILES: &str = "/(web)/{*file}";

const EMPTY_REACT_NATIVE_WEB_STYLE_ELEMENT: &str =
    "'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='";

static POLICY: LazyLock<HeaderValue> = LazyLock::new(|| {
    HeaderValue::try_from(format!(
        "default-src 'self'; script-src 'self'; style-src 'self' {EMPTY_REACT_NATIVE_WEB_STYLE_ELEMENT}; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
    ))
    .unwrap()
});

#[route(GET "/(web)")]
pub(crate) async fn index() -> topcoat::Result<Response> {
    app()
}

#[route(GET "/(web)/settings")]
pub(crate) async fn settings() -> topcoat::Result<Response> {
    app()
}

#[route(GET "/(web)/invite")]
pub(crate) async fn invite() -> topcoat::Result<Response> {
    app()
}

#[route(GET "/(web)/movies")]
pub(crate) async fn movies() -> topcoat::Result<Response> {
    app()
}

#[route(GET "/(web)/shows")]
pub(crate) async fn shows() -> topcoat::Result<Response> {
    app()
}

fn app() -> topcoat::Result<Response> {
    Err(rewrite("/index.html", Body::empty()).into())
}

pub(crate) struct ContentSecurityPolicyLayer;

impl Layer for ContentSecurityPolicyLayer {
    fn path(&self) -> Option<&Path> {
        Some(Path::new("/(web)"))
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        response_headers(cx).append(CONTENT_SECURITY_POLICY, POLICY.clone());
        next.run(cx, body)
    }
}

#[cfg(test)]
mod tests {
    use crate::test::serve_web_app;
    use http::StatusCode;
    use http::header::CONTENT_SECURITY_POLICY;
    use std::collections::HashMap;
    use test_app::TestApp;

    fn directives(policy: &str) -> HashMap<&str, Vec<&str>> {
        policy
            .split(';')
            .filter_map(|directive| {
                let mut parts = directive.split_whitespace();
                Some((parts.next()?, parts.collect()))
            })
            .collect()
    }

    #[tokio::test]
    async fn the_web_app_comes_with_a_strict_content_security_policy() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;

        let (status, headers, html) = server.get_text("/").await;

        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("<title>selfless</title>"), "{html}");
        let policy = headers[CONTENT_SECURITY_POLICY].to_str().unwrap();
        let directives = directives(policy);
        assert_eq!(directives["default-src"], ["'self'"], "{policy}");
        assert_eq!(directives["script-src"], ["'self'"], "{policy}");
        assert_eq!(directives["object-src"], ["'none'"], "{policy}");
        assert_eq!(directives["base-uri"], ["'none'"], "{policy}");
        assert_eq!(directives["frame-ancestors"], ["'none'"], "{policy}");
        assert!(!policy.contains("'unsafe-"), "{policy}");
        std::fs::remove_dir_all(web_dir).unwrap();
    }

    #[tokio::test]
    async fn the_invite_screen_loads_from_the_invite_link() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;

        let (status, headers, html) = server.get_text("/invite").await;

        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("<title>selfless</title>"), "{html}");
        assert!(headers.contains_key(CONTENT_SECURITY_POLICY));
        std::fs::remove_dir_all(web_dir).unwrap();
    }

    #[tokio::test]
    async fn the_movies_and_shows_grids_load_from_their_own_addresses() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;

        for path in ["/movies", "/shows"] {
            let (status, headers, html) = server.get_text(path).await;

            assert_eq!(status, StatusCode::OK, "{path}");
            assert!(html.contains("<title>selfless</title>"), "{path}: {html}");
            assert!(headers.contains_key(CONTENT_SECURITY_POLICY), "{path}");
        }
        std::fs::remove_dir_all(web_dir).unwrap();
    }

    #[tokio::test]
    async fn the_settings_screen_loads_from_its_own_address() {
        let app = TestApp::new().await;
        let (server, web_dir) = serve_web_app(&app).await;

        let (status, headers, html) = server.get_text("/settings").await;

        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("<title>selfless</title>"), "{html}");
        assert!(headers.contains_key(CONTENT_SECURITY_POLICY));
        std::fs::remove_dir_all(web_dir).unwrap();
    }
}
