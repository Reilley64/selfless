mod routes;

#[cfg(test)]
#[path = "../../../test/webhooks.rs"]
mod test;

use topcoat::router::RouterBuilder;

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .route(routes::webhooks::radarr::v1::radarr)
        .route(routes::webhooks::sonarr::v1::sonarr)
}
