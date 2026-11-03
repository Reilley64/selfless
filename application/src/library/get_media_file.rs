mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::AudioStream;
pub use output::MediaFileDetails;
pub use output::Output;
pub use output::SubtitleFile;
pub use output::SubtitleStream;
pub use output::VideoStream;

use crate::ports::persistence::DbScope;
use std::time::Duration;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    media_file_id: Uuid,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let Some(media_file) = db_context
        .media_files()
        .find(media_file_id)
        .include(domain::MediaFile::fields().video_streams())
        .include(domain::MediaFile::fields().audio_streams())
        .include(domain::MediaFile::fields().subtitle_streams())
        .include(domain::MediaFile::fields().subtitle_files())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output { media_file: None });
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
        .map(|x| SubtitleFile {
            path: x.path,
            language: x.language,
            hearing_impaired: x.hearing_impaired,
            forced: x.forced,
        })
        .collect::<Vec<_>>();
    subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Output {
        media_file: Some(MediaFileDetails {
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
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::HdrType;

    async fn seed(db_context: &mut DbContext) -> Uuid {
        let media_file = db_context
            .media_files()
            .create()
            .path("/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv")
            .size(Some(1_000))
            .container(Some("matroska,webm".to_owned()))
            .duration_ms(Some(6_981_000))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for index in [2, 0] {
            media_file
                .video_streams()
                .create()
                .index(index)
                .codec("hevc")
                .width(3840)
                .height(1600)
                .hdr_type(HdrType::Hdr10)
                .interlaced(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        for index in [3, 1] {
            media_file
                .audio_streams()
                .create()
                .index(index)
                .codec("truehd")
                .channels(8)
                .is_default(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        for index in [5, 4] {
            media_file
                .subtitle_streams()
                .create()
                .index(index)
                .codec("subrip")
                .is_default(false)
                .forced(false)
                .hearing_impaired(false)
                .text(true)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        for path in ["Arrival.fr.srt", "Arrival.en.srt"] {
            media_file
                .subtitle_files()
                .create()
                .path(path)
                .hearing_impaired(false)
                .forced(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        media_file.id
    }

    #[tokio::test]
    async fn a_media_file_lists_its_streams_by_index_and_its_subtitle_files_by_path() {
        let mut db_context = mock_db_context().await;
        let id = seed(&mut db_context).await;

        let media_file = call(Dependencies { db_context }, id)
            .await
            .unwrap()
            .media_file
            .unwrap();

        assert_eq!(media_file.duration, Some(Duration::from_millis(6_981_000)));
        assert_eq!(
            media_file
                .video_streams
                .iter()
                .map(|x| x.index)
                .collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(
            media_file
                .audio_streams
                .iter()
                .map(|x| x.index)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(
            media_file
                .subtitle_streams
                .iter()
                .map(|x| x.index)
                .collect::<Vec<_>>(),
            [4, 5]
        );
        assert_eq!(
            media_file
                .subtitle_files
                .iter()
                .map(|x| x.path.as_str())
                .collect::<Vec<_>>(),
            ["Arrival.en.srt", "Arrival.fr.srt"]
        );
    }
}
