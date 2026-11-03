use crate::library::sync_status::ApiCall;
use crate::webhooks::BySource;
use domain::Source;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default)]
pub struct SourceHealth(BySource<Arc<Mutex<Health>>>);

#[derive(Clone, Default)]
pub(crate) struct Health {
    pub(crate) last_api_call: Option<ApiCall>,
    pub(crate) kodi_metadata: Option<bool>,
}

impl SourceHealth {
    pub(crate) fn api_call<T, E: std::error::Error>(
        &self,
        source: Source,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.0.get(source).lock().unwrap().last_api_call = Some(match &result {
            Ok(_) => ApiCall::Succeeded,
            Err(error) => ApiCall::Failed(error.to_string()),
        });
        result
    }

    pub(crate) fn set_kodi_metadata(&self, source: Source, enabled: bool) {
        self.0.get(source).lock().unwrap().kodi_metadata = Some(enabled);
    }

    pub(crate) fn health(&self, source: Source) -> Health {
        self.0.get(source).lock().unwrap().clone()
    }
}
