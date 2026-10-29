use application::ports::ListSubtitleFiles;
use application::ports::ListSubtitleFilesError;
use application::ports::SubtitleFile;
use std::path::Path;

const EXTENSIONS: [&str; 2] = ["srt", "ass"];

pub(crate) fn list_subtitle_files() -> ListSubtitleFiles {
    ListSubtitleFiles::new(|media_file_path| Box::pin(list(media_file_path)))
}

async fn list(media_file_path: String) -> Result<Vec<SubtitleFile>, ListSubtitleFilesError> {
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

fn subtitle_file(folder: &Path, stem: &str, name: &str) -> Option<SubtitleFile> {
    let mut parts = name
        .strip_prefix(stem)?
        .strip_prefix('.')?
        .split('.')
        .collect::<Vec<_>>();
    let extension = parts.pop()?.to_ascii_lowercase();
    if !EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    let mut subtitle_file = SubtitleFile {
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
