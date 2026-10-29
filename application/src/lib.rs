pub mod accept_invite;
pub mod admin_sign_in;
pub mod admin_sign_out;
pub mod bootstrap_first_admin;
pub mod cancel_invite;
pub mod change_admin_password;
pub mod change_password;
pub mod check_database;
pub mod create_admin;
pub mod create_invite;
pub mod create_user;
mod db;
pub mod delete_admin;
pub mod delete_expired_sessions;
pub mod delete_user;
pub mod end_session;
pub mod end_user_sessions;
pub mod get_current_admin;
pub mod get_current_user;
mod invite_email;
pub mod list_pending_invites;
pub mod list_user_sessions;
mod passwords;
pub mod ports;
pub mod relay_telemetry;
pub mod resend_invite;
pub mod reset_user_password;
pub mod sign_in;
mod sign_ins;
pub mod sign_out;
mod tokens;
mod unique_index;

pub use db::DbContext;
pub use db::DbLocation;
pub use db::DbScope;
pub use db::DbSet;
pub use db::DbTransaction;
pub use passwords::EmptyPassword;

pub mod attributes {
    include!(concat!(env!("OUT_DIR"), "/attributes.rs"));
}

pub mod metrics {
    include!(concat!(env!("OUT_DIR"), "/metrics.rs"));
}

pub mod spans {
    include!(concat!(env!("OUT_DIR"), "/spans.rs"));
}

pub fn models() -> toasty::ModelSet {
    toasty::models!(domain::*)
}

#[derive(Clone)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit_id: &'static str,
    pub git_commit_time: &'static str,
}
