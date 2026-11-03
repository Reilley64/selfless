use crate::ports::Clock;
use crate::ports::persistence::DbContext;
use crate::webhooks::WebhookPasswords;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub webhook_passwords: WebhookPasswords,
}
