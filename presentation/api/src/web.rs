use topcoat::router::Body;
use topcoat::router::error::rewrite;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route(GET "/")]
pub(crate) async fn index() -> topcoat::Result<Response> {
    Err(rewrite("/index.html", Body::empty()).into())
}
