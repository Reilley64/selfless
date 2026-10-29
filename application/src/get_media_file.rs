use crate::DbContext;
use crate::ports::AudioStream;
use crate::ports::FoundSubtitleFile;
use crate::ports::SubtitleStream;
use crate::ports::VideoStream;
use std::time::Duration;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to read a media file")]
pub struct Error;

#[derive(Debug)]
pub struct MediaFileDetails {
    pub path: String,
    pub container: Option<String>,
    pub duration: Option<Duration>,
    pub size: Option<i64>,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub subtitle_files: Vec<FoundSubtitleFile>,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    media_file_id: Uuid,
) -> Result<Option<MediaFileDetails>, Error> {
    let mut db_context = dependencies.into().db_context;
    let Some(media_file) = domain::MediaFile::filter_by_id(media_file_id)
        .include(domain::MediaFile::fields().video_streams())
        .include(domain::MediaFile::fields().audio_streams())
        .include(domain::MediaFile::fields().subtitle_streams())
        .include(domain::MediaFile::fields().subtitle_files())
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(None);
    };
    let mut video_streams = media_file
        .video_streams
        .into_inner()
        .into_iter()
        .map(|x| VideoStream {
            index: x.index,
            codec: x.codec,
            profile: x.profile,
            width: x.width,
            height: x.height,
            frame_rate: x.frame_rate,
            hdr_type: x.hdr_type,
        })
        .collect::<Vec<_>>();
    video_streams.sort_by_key(|x| x.index);
    let mut audio_streams = media_file
        .audio_streams
        .into_inner()
        .into_iter()
        .map(|x| AudioStream {
            index: x.index,
            codec: x.codec,
            language: x.language,
            channels: x.channels,
            title: x.title,
            default: x.is_default,
        })
        .collect::<Vec<_>>();
    audio_streams.sort_by_key(|x| x.index);
    let mut subtitle_streams = media_file
        .subtitle_streams
        .into_inner()
        .into_iter()
        .map(|x| SubtitleStream {
            index: x.index,
            codec: x.codec,
            language: x.language,
            title: x.title,
            default: x.is_default,
            forced: x.forced,
            hearing_impaired: x.hearing_impaired,
            text: x.text,
        })
        .collect::<Vec<_>>();
    subtitle_streams.sort_by_key(|x| x.index);
    let mut subtitle_files = media_file
        .subtitle_files
        .into_inner()
        .into_iter()
        .map(|x| FoundSubtitleFile {
            path: x.path,
            language: x.language,
            hearing_impaired: x.hearing_impaired,
            forced: x.forced,
        })
        .collect::<Vec<_>>();
    subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(MediaFileDetails {
        path: media_file.path,
        container: media_file.container,
        duration: media_file
            .duration_ms
            .and_then(|x| u64::try_from(x).ok())
            .map(Duration::from_millis),
        size: media_file.size,
        video_streams,
        audio_streams,
        subtitle_streams,
        subtitle_files,
    }))
}
