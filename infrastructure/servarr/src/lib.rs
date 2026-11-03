mod webhook;

pub use webhook::read_webhook_event_type;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use tracing::Instrument;
use tracing::field;
use url::Url;

#[ohno::error]
#[display("missing setting: {name}")]
pub struct MissingSetting {
    name: &'static str,
}

pub fn setting<T: Clone>(value: &Option<T>, name: &'static str) -> Result<T, MissingSetting> {
    value.clone().ok_or_else(|| MissingSetting::new(name))
}

#[derive(Clone)]
pub struct Api {
    client: reqwest::Client,
    url: Url,
    api_key: String,
}

#[derive(Deserialize)]
struct MetadataConsumer {
    implementation: String,
    enable: bool,
}

impl Api {
    pub fn new(url: Url, api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            api_key,
        }
    }

    pub async fn kodi_metadata(&self) -> Result<bool, reqwest::Error> {
        Ok(self
            .get::<Vec<MetadataConsumer>>("/api/v3/metadata", &["metadata"], None)
            .await?
            .iter()
            .any(|x| x.implementation == "XbmcMetadata" && x.enable))
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        template: &'static str,
        path: &[&str],
        query: Option<(&str, &str)>,
    ) -> Result<T, reqwest::Error> {
        let mut url = self.url.clone();
        url.path_segments_mut()
            .expect("the URL setting is an http or https URL")
            .pop_if_empty()
            .extend(["api", "v3"])
            .extend(path);
        if let Some((name, value)) = query {
            url.query_pairs_mut().append_pair(name, value);
        }
        let span = tracing::info_span!(
            "servarr_request",
            otel.name = format!("GET {template}"),
            otel.kind = "client",
            otel.status_code = field::Empty,
            http.request.method = "GET",
            url.full = url.as_str(),
            url.template = template,
            server.address = url.host_str(),
            server.port = url.port_or_known_default().map(i64::from),
            http.response.status_code = field::Empty,
            error.type = field::Empty,
        );
        async {
            let response = self
                .client
                .get(url.clone())
                .header("X-Api-Key", &self.api_key)
                .send()
                .await?;
            span.record(
                "http.response.status_code",
                i64::from(response.status().as_u16()),
            );
            response.error_for_status()?.json::<T>().await
        }
        .instrument(span.clone())
        .await
        .inspect_err(|error| {
            span.record("otel.status_code", "ERROR");
            span.record(
                "error.type",
                error
                    .status()
                    .map_or_else(|| "_OTHER".to_owned(), |x| x.as_u16().to_string()),
            );
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Api;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    const METADATA: &str = include_str!("../../../test/fixtures/radarr/api/metadata.json");
    const API_KEY: &str = "api-key";

    async fn serve_metadata(response: ResponseTemplate) -> (MockServer, Api) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/metadata"))
            .respond_with(response)
            .mount(&server)
            .await;
        let api = Api::new(server.uri().parse().unwrap(), API_KEY.to_owned());
        (server, api)
    }

    #[tokio::test]
    async fn kodi_metadata_is_on_only_for_an_enabled_kodi_consumer() {
        let (_server, api) =
            serve_metadata(ResponseTemplate::new(200).set_body_raw(METADATA, "application/json"))
                .await;
        assert!(api.kodi_metadata().await.unwrap());

        let (_server, api) = serve_metadata(ResponseTemplate::new(200).set_body_raw(
            r#"[
                {"implementation": "MediaBrowserMetadata", "enable": true},
                {"implementation": "XbmcMetadata", "enable": false}
            ]"#,
            "application/json",
        ))
        .await;
        assert!(!api.kodi_metadata().await.unwrap());
    }

    #[tokio::test]
    async fn requests_send_the_api_key() {
        let (server, api) =
            serve_metadata(ResponseTemplate::new(200).set_body_raw(METADATA, "application/json"))
                .await;

        api.kodi_metadata().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].headers["X-Api-Key"], API_KEY);
    }

    #[tokio::test]
    async fn a_non_success_status_is_an_error() {
        for status in [401, 500] {
            let (_server, api) = serve_metadata(ResponseTemplate::new(status)).await;

            assert!(api.kodi_metadata().await.is_err(), "{status}");
        }
    }
}
