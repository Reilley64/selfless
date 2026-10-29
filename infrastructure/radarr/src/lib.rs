use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct RadarrSettings {
    pub webhook_password: Option<String>,
}

#[ohno::error]
#[display("missing setting: RADARR__WEBHOOK_PASSWORD")]
pub struct MissingWebhookPassword;

impl RadarrSettings {
    pub fn webhook_password(&self) -> Result<String, MissingWebhookPassword> {
        self.webhook_password
            .clone()
            .ok_or_else(MissingWebhookPassword::new)
    }
}
