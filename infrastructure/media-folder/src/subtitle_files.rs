use application::ports::FoundSubtitleFile;
use application::ports::ListSubtitleFiles;
use application::ports::ListSubtitleFilesError;
use std::path::Path;

const EXTENSIONS: [&str; 2] = ["srt", "ass"];

pub fn list_subtitle_files() -> ListSubtitleFiles {
    ListSubtitleFiles::new(|media_file_path| Box::pin(list(media_file_path)))
}

async fn list(media_file_path: String) -> Result<Vec<FoundSubtitleFile>, ListSubtitleFilesError> {
    let media_file_path = Path::new(&media_file_path);
    let (Some(folder), Some(stem)) = (
        media_file_path.parent(),
        media_file_path.file_stem().and_then(|x| x.to_str()),
    ) else {
        return Ok(Vec::new());
    };
    let mut entries = tokio::fs::read_dir(folder)
        .await
        .map_err(ListSubtitleFilesError::caused_by)?;
    let mut subtitle_files = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(ListSubtitleFilesError::caused_by)?
    {
        if let Some(name) = entry.file_name().to_str()
            && let Some(subtitle_file) = subtitle_file(folder, stem, name)
        {
            subtitle_files.push(subtitle_file);
        }
    }
    subtitle_files.sort();
    Ok(subtitle_files)
}

fn subtitle_file(folder: &Path, stem: &str, name: &str) -> Option<FoundSubtitleFile> {
    let mut parts = name
        .strip_prefix(stem)?
        .strip_prefix('.')?
        .split('.')
        .collect::<Vec<_>>();
    let extension = parts.pop()?.to_ascii_lowercase();
    if !EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    let mut subtitle_file = FoundSubtitleFile {
        path: folder.join(name).to_str()?.to_owned(),
        language: None,
        hearing_impaired: false,
        forced: false,
    };
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "hi" | "sdh" => subtitle_file.hearing_impaired = true,
            "forced" => subtitle_file.forced = true,
            tag if subtitle_file.language.is_none() => {
                subtitle_file.language = Some(language(tag)?);
            }
            _ => return None,
        }
    }
    Some(subtitle_file)
}

fn language(tag: &str) -> Option<String> {
    let (code, region) = match tag.split_once('-') {
        Some((code, region)) => (code, Some(region)),
        None => (tag, None),
    };
    let letters = |x: &str| x.bytes().all(|x| x.is_ascii_lowercase());
    if !(2..=3).contains(&code.len()) || !letters(code) {
        return None;
    }
    match region {
        None => Some(code.to_owned()),
        Some(region) if region.len() == 2 && letters(region) => {
            Some(format!("{code}-{}", region.to_ascii_uppercase()))
        }
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::list_subtitle_files;
    use application::ports::FoundSubtitleFile;
    use tempfile::TempDir;

    const VIDEO: &str = "Arrival (2016) Bluray-1080p";

    #[tokio::test]
    async fn subtitle_files_count_when_their_name_is_the_video_name_then_a_language_and_flags() {
        let folder = TempDir::new().unwrap();
        let path = |name: &str| folder.path().join(name).to_str().unwrap().to_owned();
        for name in [
            format!("{VIDEO}.mkv"),
            format!("{VIDEO}.en.srt"),
            format!("{VIDEO}.en.hi.srt"),
            format!("{VIDEO}.hi.srt"),
            format!("{VIDEO}.fr.forced.ass"),
            format!("{VIDEO}.pt-br.srt"),
            format!("{VIDEO}.en.sdh.srt"),
            format!("{VIDEO}.sdh.srt"),
            format!("{VIDEO}.nfo"),
            format!("{VIDEO}.en.commentary.srt"),
            "Arrival (2016) WEBDL-720p.en.srt".to_owned(),
        ] {
            std::fs::write(path(&name), []).unwrap();
        }
        let subtitle_file =
            |suffix: &str, language: Option<&str>, hearing_impaired, forced| FoundSubtitleFile {
                path: path(&format!("{VIDEO}{suffix}")),
                language: language.map(str::to_owned),
                hearing_impaired,
                forced,
            };

        let subtitle_files = list_subtitle_files()
            .call(&path(&format!("{VIDEO}.mkv")))
            .await
            .unwrap();

        assert_eq!(
            subtitle_files,
            [
                subtitle_file(".en.hi.srt", Some("en"), true, false),
                subtitle_file(".en.sdh.srt", Some("en"), true, false),
                subtitle_file(".en.srt", Some("en"), false, false),
                subtitle_file(".fr.forced.ass", Some("fr"), false, true),
                subtitle_file(".hi.srt", None, true, false),
                subtitle_file(".pt-br.srt", Some("pt-BR"), false, false),
                subtitle_file(".sdh.srt", None, true, false),
            ]
        );
    }
}
