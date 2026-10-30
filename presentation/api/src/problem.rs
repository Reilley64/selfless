use application::invites::accept_invite::InviteNotFound;
use application::users::change_password::WrongCurrentPassword;
use application::users::create_user::NameTaken;
use ohno::ErrorExt;
use serde_json::json;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::router::Body;
use topcoat::router::Layer;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::Path;
use topcoat::router::StatusCode;
use topcoat::router::error::BadRequestError;
use topcoat::router::error::RewriteError;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::try_endpoint;

pub(crate) struct Problem {
    problem_type: &'static str,
    title: &'static str,
    status: StatusCode,
}

pub(crate) fn from_error<E>(cx: &Cx, error: E) -> topcoat::Result<Response>
where
    E: ErrorExt + Into<topcoat::Error>,
{
    if error.find_source::<WrongCurrentPassword>().is_some() {
        WRONG_CURRENT_PASSWORD.into_response(cx)
    } else if error.find_source::<InviteNotFound>().is_some() {
        INVITE_NOT_FOUND.into_response(cx)
    } else if error.find_source::<NameTaken>().is_some() {
        NAME_TAKEN.into_response(cx)
    } else {
        Err(error.into())
    }
}

const BAD_REQUEST: Problem = Problem {
    problem_type: "urn:selfless:problem:bad-request",
    title: "The request body is not valid.",
    status: StatusCode::BAD_REQUEST,
};

pub(crate) const INVALID_IDEMPOTENCY_KEY: Problem = Problem {
    problem_type: "urn:selfless:problem:invalid-idempotency-key",
    title: "The Idempotency-Key header is not valid.",
    status: StatusCode::BAD_REQUEST,
};

const INTERNAL_ERROR: Problem = Problem {
    problem_type: "urn:selfless:problem:internal-error",
    title: "Something went wrong on the server.",
    status: StatusCode::INTERNAL_SERVER_ERROR,
};

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

const WRONG_CURRENT_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:wrong-current-password",
    title: "The current password is wrong.",
    status: StatusCode::FORBIDDEN,
};

pub(crate) const EMPTY_PASSWORD: Problem = Problem {
    problem_type: "urn:selfless:problem:empty-password",
    title: "The password is empty.",
    status: StatusCode::UNPROCESSABLE_ENTITY,
};

const INVITE_NOT_FOUND: Problem = Problem {
    problem_type: "urn:selfless:problem:invite-not-found",
    title: "The invite link no longer works.",
    status: StatusCode::NOT_FOUND,
};

const NAME_TAKEN: Problem = Problem {
    problem_type: "urn:selfless:problem:name-taken",
    title: "A user already has this name.",
    status: StatusCode::CONFLICT,
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

pub(crate) struct ProblemLayer;

impl Layer for ProblemLayer {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let error = match next.run(cx, body).await {
                Err(error) if !error.is::<RewriteError>() => error,
                result => return result,
            };
            let api = try_endpoint(cx).is_some_and(|x| x.path().starts_with(Path::new("/api")));
            if api && error.is::<BadRequestError>() {
                return BAD_REQUEST.into_response(cx);
            }
            if error.clone().into_response(cx)?.status() != StatusCode::INTERNAL_SERVER_ERROR {
                return Err(error);
            }
            tracing::error!(error = &*error as &dyn Error, "request failed");
            if api {
                INTERNAL_ERROR.into_response(cx)
            } else {
                Err(error)
            }
        })
    }
}
