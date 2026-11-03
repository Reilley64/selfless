mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::playback::TrackIds;
use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Playback;
use domain::PlaybackSettings;
use domain::SubtitleTrack;
use domain::TrackLanguage;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    playback_id: Uuid,
    position: f64,
    paused: bool,
    stopped: bool,
    tracks: TrackIds,
) -> Result<Output, Error> {
    let TrackIds {
        audio: audio_track_id,
        subtitle: subtitle_track_id,
    } = tracks;
    let Dependencies {
        mut db_context,
        clock,
        end_hls_output,
    } = dependencies.into();
    let Some(mut playback) = db_context
        .playbacks()
        .filter(
            Playback::fields()
                .id()
                .eq(playback_id)
                .and(Playback::fields().user_id().eq(user_id)),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    if playback.ended() {
        return Ok(Output::Ended);
    }
    let audio_changed = playback.audio_track_id != audio_track_id;
    let subtitle_changed = playback.subtitle_track_id != subtitle_track_id;
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    if audio_changed || subtitle_changed {
        let media_file = transaction
            .media_files()
            .find(playback.media_file_id)
            .include(MediaFile::fields().audio_streams())
            .include(MediaFile::fields().subtitle_streams())
            .include(MediaFile::fields().subtitle_files())
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        let audio_language = media_file
            .iter()
            .flat_map(|x| x.audio_streams.get())
            .find(|x| audio_changed && Some(x.id) == audio_track_id)
            .and_then(|x| x.language.as_deref())
            .map(TrackLanguage::remembered);
        let subtitle_language = match subtitle_track_id {
            _ if !subtitle_changed => None,
            None => Some(None),
            Some(id) => media_file
                .as_ref()
                .and_then(|x| {
                    SubtitleTrack::all(x.subtitle_streams.get(), x.subtitle_files.get())
                        .into_iter()
                        .find(|x| x.id() == id)
                })
                .and_then(|x| x.language())
                .map(|x| Some(TrackLanguage::remembered(x))),
        };
        if audio_language.is_some() || subtitle_language.is_some() {
            let mut settings = PlaybackSettings::upsert_by_user_id(user_id);
            if let Some(language) = audio_language {
                settings = settings.audio_language(Some(language));
            }
            if let Some(language) = subtitle_language {
                settings = settings.subtitle_language(language);
            }
            settings
                .exec(transaction.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
    }
    playback
        .update()
        .position(position)
        .paused(paused)
        .audio_track_id(audio_track_id)
        .subtitle_track_id(subtitle_track_id)
        .last_reported_at(now)
        .ended_at(stopped.then_some(now))
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    if stopped {
        end_hls_output.call(playback_id).await;
    }
    Ok(Output::Reported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::EndedHlsOutputs;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;

    const STARTED_AT: &str = "2026-11-03T20:00:00Z";
    const REPORTED_AT: &str = "2026-11-03T20:00:10Z";

    struct Fixture {
        db_context: DbContext,
        user_id: Uuid,
        playback_id: Uuid,
        english_audio: Uuid,
        japanese_audio: Uuid,
        english_subtitle: Uuid,
        french_subtitle: Uuid,
        ended: EndedHlsOutputs,
    }

    impl Fixture {
        async fn new() -> Self {
            let mut db_context = mock_db_context().await;
            let media_file = db_context
                .media_files()
                .create()
                .path("/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv")
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            let mut audio = Vec::new();
            for (index, language) in [(1, "eng"), (2, "jpn")] {
                audio.push(
                    media_file
                        .audio_streams()
                        .create()
                        .index(index)
                        .codec("aac")
                        .language(Some(language.to_owned()))
                        .channels(2)
                        .is_default(index == 1)
                        .exec(db_context.executor_mut())
                        .await
                        .unwrap()
                        .id,
                );
            }
            let english_subtitle = media_file
                .subtitle_streams()
                .create()
                .index(3)
                .codec("subrip")
                .language(Some("eng".to_owned()))
                .is_default(false)
                .forced(false)
                .hearing_impaired(false)
                .text(true)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            let french_subtitle = media_file
                .subtitle_files()
                .create()
                .path("/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.fr.srt")
                .language(Some("fr".to_owned()))
                .hearing_impaired(false)
                .forced(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            let user_id = Uuid::new_v4();
            let playback_id = playback(&db_context, at(STARTED_AT))
                .user_id(user_id)
                .media_file_id(media_file.id)
                .audio_track_id(Some(audio[0]))
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            Self {
                db_context,
                user_id,
                playback_id,
                english_audio: audio[0],
                japanese_audio: audio[1],
                english_subtitle,
                french_subtitle,
                ended: EndedHlsOutputs::default(),
            }
        }

        async fn report(&self, report: Report) -> Output {
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(|| at(REPORTED_AT)),
                    end_hls_output: self.ended.port(),
                },
                report.user_id.unwrap_or(self.user_id),
                self.playback_id,
                30.0,
                report.paused,
                report.stopped,
                TrackIds {
                    audio: report.audio_track_id.or(Some(self.english_audio)),
                    subtitle: report.subtitle_track_id,
                },
            )
            .await
            .unwrap()
        }

        async fn remembered(&self) -> (Option<String>, Option<String>) {
            let mut db_context = self.db_context.clone();
            db_context
                .playback_settings()
                .find(self.user_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .map_or((None, None), |x| (x.audio_language, x.subtitle_language))
        }

        async fn stored(&self) -> Playback {
            let mut db_context = self.db_context.clone();
            db_context
                .playbacks()
                .find(self.playback_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .unwrap()
        }
    }

    #[derive(Default)]
    struct Report {
        user_id: Option<Uuid>,
        paused: bool,
        stopped: bool,
        audio_track_id: Option<Uuid>,
        subtitle_track_id: Option<Uuid>,
    }

    #[tokio::test]
    async fn a_report_stores_the_position_paused_state_and_report_time() {
        let fixture = Fixture::new().await;

        fixture
            .report(Report {
                paused: true,
                ..Report::default()
            })
            .await;

        let stored = fixture.stored().await;
        assert_eq!(
            (stored.position, stored.paused, stored.last_reported_at),
            (30.0, true, at(REPORTED_AT))
        );
        assert_eq!(stored.ended_at, None);
    }

    #[tokio::test]
    async fn a_report_with_other_tracks_changes_the_remembered_languages_to_match() {
        let fixture = Fixture::new().await;

        fixture.report(Report::default()).await;
        let unchanged = fixture.remembered().await;
        fixture
            .report(Report {
                audio_track_id: Some(fixture.japanese_audio),
                subtitle_track_id: Some(fixture.french_subtitle),
                ..Report::default()
            })
            .await;
        let changed = fixture.remembered().await;
        fixture
            .report(Report {
                audio_track_id: Some(fixture.japanese_audio),
                ..Report::default()
            })
            .await;
        let subtitles_off = fixture.remembered().await;

        assert_eq!(unchanged, (None, None));
        assert_eq!(changed, (Some("jpn".to_owned()), Some("fra".to_owned())));
        assert_eq!(subtitles_off, (Some("jpn".to_owned()), None));
    }

    #[tokio::test]
    async fn the_first_report_with_tracks_other_than_the_stored_defaults_remembers_their_languages()
    {
        let fixture = Fixture::new().await;

        fixture
            .report(Report {
                subtitle_track_id: Some(fixture.english_subtitle),
                ..Report::default()
            })
            .await;

        assert_eq!(fixture.remembered().await, (None, Some("eng".to_owned())));
    }

    #[tokio::test]
    async fn a_stop_ends_the_playback_and_its_hls_output_and_a_later_report_finds_it_ended() {
        let fixture = Fixture::new().await;

        let stop = fixture
            .report(Report {
                stopped: true,
                ..Report::default()
            })
            .await;
        let after_stop = fixture.report(Report::default()).await;

        assert!(matches!(stop, Output::Reported), "{stop:?}");
        assert!(matches!(after_stop, Output::Ended), "{after_stop:?}");
        assert_eq!(fixture.stored().await.ended_at, Some(at(REPORTED_AT)));
        assert_eq!(fixture.ended.playback_ids(), [fixture.playback_id]);
    }

    #[tokio::test]
    async fn another_users_report_is_not_found() {
        let fixture = Fixture::new().await;

        let output = fixture
            .report(Report {
                user_id: Some(Uuid::new_v4()),
                ..Report::default()
            })
            .await;

        assert!(matches!(output, Output::NotFound), "{output:?}");
        assert_eq!(fixture.stored().await.last_reported_at, at(STARTED_AT));
    }
}
