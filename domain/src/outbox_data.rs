use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, toasty::Embed)]
pub enum OutboxData {
    SendInviteEmail { invite_id: Uuid, origin: String },
}
