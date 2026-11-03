use isolang::Language;

const BIBLIOGRAPHIC_CODES: [(&str, &str); 20] = [
    ("alb", "sqi"),
    ("arm", "hye"),
    ("baq", "eus"),
    ("bur", "mya"),
    ("chi", "zho"),
    ("cze", "ces"),
    ("dut", "nld"),
    ("fre", "fra"),
    ("geo", "kat"),
    ("ger", "deu"),
    ("gre", "ell"),
    ("ice", "isl"),
    ("mac", "mkd"),
    ("mao", "mri"),
    ("may", "msa"),
    ("per", "fas"),
    ("rum", "ron"),
    ("slo", "slk"),
    ("tib", "bod"),
    ("wel", "cym"),
];

pub(crate) struct TrackLanguage<'a> {
    language: Language,
    region: Option<&'a str>,
}

impl<'a> TrackLanguage<'a> {
    pub(crate) fn parse(code: &'a str) -> Option<Self> {
        let (primary, region) = match code.split_once('-') {
            Some((primary, region)) => (primary, Some(region)),
            None => (code, None),
        };
        let primary = BIBLIOGRAPHIC_CODES
            .iter()
            .find(|(bibliographic, _)| *bibliographic == primary)
            .map_or(primary, |(_, terminologic)| terminologic);
        let language = Language::from_639_1(primary).or_else(|| Language::from_639_3(primary))?;
        Some(Self { language, region })
    }

    pub(crate) fn name(&self) -> &'static str {
        let name = self.language.to_name();
        name.split_once(" (").map_or(name, |(name, _)| name)
    }

    pub(crate) fn region(&self) -> Option<&'a str> {
        self.region
    }

    pub(crate) fn tag(&self) -> String {
        let primary = self
            .language
            .to_639_1()
            .unwrap_or_else(|| self.language.to_639_3());
        match self.region {
            Some(region) => format!("{primary}-{region}"),
            None => primary.to_owned(),
        }
    }
}
