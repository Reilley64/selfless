pub mod check_database;

#[derive(Clone)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit_id: &'static str,
    pub git_commit_time: &'static str,
}

#[derive(Clone)]
pub struct DbContext {
    db: toasty::Db,
}

impl DbContext {
    pub fn new(db: toasty::Db) -> Self {
        Self { db }
    }

    pub fn executor_mut(&mut self) -> &mut dyn toasty::Executor {
        &mut self.db
    }
}
