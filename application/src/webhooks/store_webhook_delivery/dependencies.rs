use crate::ports::Clock;
use crate::ports::ReadWebhookEventType;
use crate::ports::persistence::DbContext;
use crate::webhooks::WebhookPasswords;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub read_webhook_event_type: ReadWebhookEventType,
    pub webhook_passwords: WebhookPasswords,
}
