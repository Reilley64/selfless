use crate::Availability;
use infrastructure::AppState;
use serde::Serialize;
use std::collections::BTreeMap;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::content::Json;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[derive(Serialize)]
struct Info {
    build: Build,
    git: Git,
}

#[derive(Serialize)]
struct Build {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct Git {
    branch: &'static str,
    commit: Commit,
}

#[derive(Serialize)]
struct Commit {
    id: &'static str,
    time: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Status {
    Down,
    OutOfService,
    Up,
}

#[derive(Serialize)]
struct Component {
    status: Status,
}

#[derive(Serialize)]
struct Health {
    status: Status,
    components: BTreeMap<&'static str, Component>,
}

#[route(GET "/actuator/health/v1")]
pub(crate) async fn health(cx: &Cx) -> topcoat::Result<Response> {
    respond(
        cx,
        [
            ("db", database_status(cx).await),
            ("livenessState", Status::Up),
            ("readinessState", readiness_state(cx)),
        ],
    )
}

#[route(GET "/actuator/health/liveness/v1")]
pub(crate) async fn liveness(cx: &Cx) -> topcoat::Result<Response> {
    respond(cx, [("livenessState", Status::Up)])
}

#[route(GET "/actuator/health/readiness/v1")]
pub(crate) async fn readiness(cx: &Cx) -> topcoat::Result<Response> {
    respond(
        cx,
        [
            ("db", database_status(cx).await),
            ("readinessState", readiness_state(cx)),
        ],
    )
}

#[route(GET "/actuator/info/v1")]
pub(crate) async fn info(cx: &Cx) -> topcoat::Result<Response> {
    let build_info = &app_context::<AppState>(cx).build_info;
    Json(Info {
        build: Build {
            name: build_info.name,
            version: build_info.version,
        },
        git: Git {
            branch: build_info.git_branch,
            commit: Commit {
                id: build_info.git_commit_id,
                time: build_info.git_commit_time,
            },
        },
    })
    .into_response(cx)
}

async fn database_status(cx: &Cx) -> Status {
    match application::check_database::call(app_context::<AppState>(cx)).await {
        Ok(()) => Status::Up,
        Err(error) => {
            tracing::warn!(error = &error as &dyn Error, "database check failed");
            Status::Down
        }
    }
}

fn readiness_state(cx: &Cx) -> Status {
    if app_context::<Availability>(cx).is_accepting_traffic() {
        Status::Up
    } else {
        Status::OutOfService
    }
}

fn respond<const N: usize>(
    cx: &Cx,
    components: [(&'static str, Status); N],
) -> topcoat::Result<Response> {
    let status = components
        .iter()
        .map(|(_, x)| *x)
        .min()
        .unwrap_or(Status::Up);
    let status_code = match status {
        Status::Up => StatusCode::OK,
        Status::Down | Status::OutOfService => StatusCode::SERVICE_UNAVAILABLE,
    };
    let components = components
        .into_iter()
        .map(|(name, status)| (name, Component { status }))
        .collect();
    (status_code, Json(Health { status, components })).into_response(cx)
}
