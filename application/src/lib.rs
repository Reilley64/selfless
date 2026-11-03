mod passwords;
mod tokens;

pub mod actuator;
pub mod admins;
pub mod browsing;
pub mod invites;
pub mod library;
pub mod ports;
pub mod sessions;
pub mod telemetry;
pub mod trace_context;
pub mod url_signer;
pub mod users;
pub mod webhooks;

#[cfg(test)]
#[path = "../../test/application.rs"]
mod test;

pub mod attributes {
    include!(concat!(env!("OUT_DIR"), "/attributes.rs"));
}

pub mod metrics {
    include!(concat!(env!("OUT_DIR"), "/metrics.rs"));
}

pub mod spans {
    include!(concat!(env!("OUT_DIR"), "/spans.rs"));
}

#[derive(Clone)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit_id: &'static str,
    pub git_commit_time: &'static str,
}
