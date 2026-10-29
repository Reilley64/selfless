use serde_json::json;
use topcoat::context::Cx;
use topcoat::router::StatusCode;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;

pub(crate) struct Problem {
    problem_type: &'static str,
    title: &'static str,
    status: StatusCode,
}

pub(crate) const WRONG_NAME_OR_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:wrong-name-or-password",
    title: "The name or password is wrong.",
    status: StatusCode::UNAUTHORIZED,
};

pub(crate) const UNAUTHENTICATED: Problem = Problem {
    problem_type: "urn:selfless:problem:unauthenticated",
    title: "The request has no valid session token.",
    status: StatusCode::UNAUTHORIZED,
};

impl IntoResponse for Problem {
    fn into_response(self, cx: &Cx) -> topcoat::Result<Response> {
        (
            self.status,
            [(CONTENT_TYPE, "application/problem+json")],
            json!({
                "type": self.problem_type,
                "title": self.title,
                "status": self.status.as_u16(),
            })
            .to_string(),
        )
            .into_response(cx)
    }
}
