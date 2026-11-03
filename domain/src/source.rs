#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum Source {
    Radarr,
    Sonarr,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Radarr => "radarr",
            Source::Sonarr => "sonarr",
        }
    }
}
