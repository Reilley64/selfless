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
