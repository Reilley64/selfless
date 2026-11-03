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

pub struct TrackLanguage<'a> {
    language: Language,
    region: Option<&'a str>,
}

impl<'a> TrackLanguage<'a> {
    pub fn parse(code: &'a str) -> Option<Self> {
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

    pub fn name(&self) -> &'static str {
        let name = self.language.to_name();
        name.split_once(" (").map_or(name, |(name, _)| name)
    }

    pub fn region(&self) -> Option<&'a str> {
        self.region
    }

    pub fn remembered(code: &str) -> String {
        TrackLanguage::parse(code)
            .map_or_else(|| code.to_lowercase(), |x| x.language.to_639_3().to_owned())
    }

    pub fn matches(code: Option<&str>, remembered: &str) -> bool {
        code.is_some_and(|x| Self::remembered(x) == remembered)
    }

    pub fn tag(&self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_language_is_remembered_as_its_iso_639_3_code_without_its_region() {
        let remembered =
            ["en", "eng", "en-US", "fre", "fra", "pt-BR", "xx"].map(TrackLanguage::remembered);

        assert_eq!(remembered, ["eng", "eng", "eng", "fra", "fra", "por", "xx"]);
    }

    #[test]
    fn a_track_matches_a_remembered_language_whichever_code_it_is_tagged_with() {
        assert!(TrackLanguage::matches(Some("en"), "eng"));
        assert!(TrackLanguage::matches(Some("ger"), "deu"));
        assert!(!TrackLanguage::matches(Some("fr"), "eng"));
        assert!(!TrackLanguage::matches(None, "eng"));
    }
}
