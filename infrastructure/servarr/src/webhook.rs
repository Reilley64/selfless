use application::ports::ReadWebhookEventType;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventType {
    event_type: String,
}

pub fn read_webhook_event_type() -> ReadWebhookEventType {
    ReadWebhookEventType::new(|body| {
        serde_json::from_slice::<EventType>(body)
            .ok()
            .map(|x| x.event_type)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_type_is_read_from_a_body_selfless_cannot_otherwise_read() {
        let event_type = read_webhook_event_type().call(br#"{"eventType":"Download"}"#);

        assert_eq!(event_type.as_deref(), Some("Download"));
    }
}
