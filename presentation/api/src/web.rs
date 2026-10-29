use topcoat::context::Cx;
use topcoat::router::Body;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::error::rewrite;
use topcoat::router::header::CONTENT_SECURITY_POLICY;
use topcoat::router::header::HeaderValue;
use topcoat::router::response::Response;
use topcoat::router::response::response_headers;
use topcoat::router::route;

const STRICT_CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-hashes' 'sha256-4AXVd4Ue0A6d/wfNkdhs1EwnaIBGk3lC7Q5dkpfGJEA='; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

#[route(GET "/")]
pub(crate) async fn index() -> topcoat::Result<Response> {
    Err(rewrite("/index.html", Body::empty()).into())
}

pub(crate) fn strict_content_security_policy<'a>(
    cx: &'a Cx,
    body: Body,
    next: Next<'a>,
) -> LayerFuture<'a> {
    response_headers(cx).append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(STRICT_CONTENT_SECURITY_POLICY),
    );
    next.run(cx, body)
}
