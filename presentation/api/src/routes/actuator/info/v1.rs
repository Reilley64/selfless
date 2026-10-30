use infrastructure::AppState;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
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
