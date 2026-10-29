use crate::DbContext;
use crate::ports::AudioStream;
use crate::ports::SubtitleFile;
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
pub struct MediaFile {
    pub path: String,
    pub container: Option<String>,
    pub duration: Option<Duration>,
    pub size: Option<i64>,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub subtitle_files: Vec<SubtitleFile>,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    media_file_id: Uuid,
) -> Result<Option<MediaFile>, Error> {
    let mut db_context = dependencies.into().db_context;
    let db = db_context.executor_mut();
    let Some(media_file) = domain::MediaFile::filter_by_id(media_file_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(None);
    };
    let video_streams = domain::VideoStream::filter_by_media_file_id(media_file_id)
        .order_by(domain::VideoStream::fields().index().asc())
        .exec(db)
        .await
        .map_err(Error::caused_by)?
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
        .collect();
    let audio_streams = domain::AudioStream::filter_by_media_file_id(media_file_id)
        .order_by(domain::AudioStream::fields().index().asc())
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| AudioStream {
            index: x.index,
            codec: x.codec,
            language: x.language,
            channels: x.channels,
            title: x.title,
            default: x.is_default,
        })
        .collect();
    let subtitle_streams = domain::SubtitleStream::filter_by_media_file_id(media_file_id)
        .order_by(domain::SubtitleStream::fields().index().asc())
        .exec(db)
        .await
        .map_err(Error::caused_by)?
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
        .collect();
    let subtitle_files = domain::SubtitleFile::filter_by_media_file_id(media_file_id)
        .order_by(domain::SubtitleFile::fields().path().asc())
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| SubtitleFile {
            path: x.path,
            language: x.language,
            hearing_impaired: x.hearing_impaired,
            forced: x.forced,
        })
        .collect();
    Ok(Some(MediaFile {
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
