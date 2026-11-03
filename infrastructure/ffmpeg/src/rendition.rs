use crate::language::TrackLanguage;

pub(crate) struct Rendition<'a> {
    pub(crate) language: Option<&'a str>,
    pub(crate) title: Option<&'a str>,
    pub(crate) forced: bool,
    pub(crate) hearing_impaired: bool,
}

pub(crate) struct NamedRendition {
    pub(crate) name: String,
    pub(crate) language: Option<String>,
    pub(crate) forced: bool,
}

pub(crate) fn named<'a>(renditions: impl Iterator<Item = Rendition<'a>>) -> Vec<NamedRendition> {
    let mut named = Vec::<NamedRendition>::new();
    for rendition in renditions {
        let language = rendition.language.and_then(TrackLanguage::parse);
        let base = name(&rendition, language.as_ref());
        let mut name = base.clone();
        let mut count = 1;
        while named.iter().any(|x| x.name == name) {
            count += 1;
            name = format!("{base} {count}");
        }
        named.push(NamedRendition {
            name,
            language: language.map(|x| x.tag()),
            forced: rendition.forced,
        });
    }
    named
}

fn name(rendition: &Rendition, language: Option<&TrackLanguage>) -> String {
    let language_name = match (language, rendition.language) {
        (Some(language), _) => language.name(),
        (None, Some(code)) => code,
        (None, None) => "Unknown",
    };
    let mut marks = Vec::new();
    marks.extend(language.and_then(TrackLanguage::region));
    marks.extend(
        rendition
            .title
            .filter(|x| !x.eq_ignore_ascii_case(language_name)),
    );
    if rendition.hearing_impaired {
        marks.push("SDH");
    }
    if rendition.forced {
        marks.push("Forced");
    }
    let name = if marks.is_empty() {
        language_name.to_owned()
    } else {
        format!("{language_name} ({})", marks.join(", "))
    };
    name.replace('"', "'")
}
