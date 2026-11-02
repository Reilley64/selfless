use jiff::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
pub enum InviteEmailStatus {
    Queued { queued_at: Timestamp },
    Sent { sent_at: Timestamp },
    Rejected { rejected_at: Timestamp },
}
