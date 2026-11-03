use domain::Source;

#[derive(Clone, Default)]
pub struct BySource<T> {
    pub radarr: T,
    pub sonarr: T,
}

impl<T> BySource<T> {
    pub fn get(&self, source: Source) -> &T {
        match source {
            Source::Radarr => &self.radarr,
            Source::Sonarr => &self.sonarr,
        }
    }
}

pub type WebhookPasswords = BySource<String>;
