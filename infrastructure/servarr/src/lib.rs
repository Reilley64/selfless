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
            server.port = url.port_or_known_default(),
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
            span.record("http.response.status_code", response.status().as_u16());
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
