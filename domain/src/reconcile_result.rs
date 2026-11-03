#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum ReconcileResult {
    Succeeded,
    Failed,
}

impl ReconcileResult {
    pub fn name(self) -> &'static str {
        match self {
            ReconcileResult::Succeeded => "succeeded",
            ReconcileResult::Failed => "failed",
        }
    }
}
