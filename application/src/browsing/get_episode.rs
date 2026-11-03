mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Episode;
pub use output::Output;
pub use output::Show;

use crate::browsing::Image;
use crate::browsing::episode_watch_states;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::MediaFile;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    episode_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        url_signer,
    } = dependencies.into();
    let Some(episode) = db_context
        .episodes()
        .filter(
            domain::Episode::fields()
                .id()
                .eq(episode_id)
                .and(domain::Episode::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    let season = db_context
        .seasons()
        .filter(domain::Season::fields().id().eq(episode.season_id))
        .get(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let show = db_context
        .shows()
        .filter(domain::Show::fields().id().eq(season.show_id))
        .get(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let media_file = match episode.media_file_id {
        Some(media_file_id) => db_context
            .media_files()
            .find(media_file_id)
            .include(MediaFile::fields().audio_streams())
            .include(MediaFile::fields().subtitle_streams())
            .include(MediaFile::fields().subtitle_files())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?,
        None => None,
    };
    let duration_ms = media_file.as_ref().and_then(|x| x.duration_ms);
    let state = episode_watch_states(&mut db_context, user_id, vec![episode.id])
        .await
        .map_err(Error::caused_by)?
        .remove(&episode.id)
        .map(|x| x.watch_state())
        .unwrap_or_default();
    let now = clock.call();
    let thumb = db_context
        .artwork()
        .filter(
            Artwork::fields()
                .episode_id()
                .eq(episode.id)
                .and(Artwork::fields().kind().is_thumb()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .map(|x| Image::new(&x, &url_signer, now));
    let logo = db_context
        .artwork()
        .filter(
            Artwork::fields()
                .show_id()
                .eq(show.id)
                .and(Artwork::fields().kind().is_clearlogo()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .map(|x| Image::new(&x, &url_signer, now));
    Ok(Output::Episode(Box::new(Episode {
        id: episode.id,
        title: episode.title,
        season_number: season.number,
        episode_number: episode.number,
        air_date: episode.air_date,
        runtime_minutes: media_file.as_ref().and_then(MediaFile::runtime_minutes),
        rating: episode.rating,
        plot: episode.overview,
        thumb,
        show: Show {
            id: show.id,
            title: show.title,
            logo,
        },
        audio_languages: media_file
            .as_ref()
            .map(MediaFile::audio_languages)
            .unwrap_or_default(),
        subtitle_languages: media_file
            .as_ref()
            .map(MediaFile::subtitle_languages)
            .unwrap_or_default(),
        resume_position_seconds: state.resume_position_seconds,
        progress: state.progress(duration_ms),
        minutes_left: state.minutes_left(duration_ms),
        watched: state.watched,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::Image;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use crate::test::stop_at;
    use crate::test::url_signer;
    use crate::test::watch_to_the_end;
    use domain::ArtworkKind;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use jiff::civil::date;
    use test_fixtures::FakeClock;

    fn dependencies(db_context: &DbContext) -> Dependencies {
        Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
            url_signer: url_signer(),
        }
    }

    async fn get(db_context: &DbContext, episode_id: Uuid) -> Output {
        call(dependencies(db_context), Uuid::new_v4(), episode_id)
            .await
            .unwrap()
    }

    async fn get_episode(db_context: &DbContext, episode_id: Uuid) -> Episode {
        match get(db_context, episode_id).await {
            Output::Episode(episode) => *episode,
            Output::NotFound => panic!("the episode is not found"),
        }
    }

    async fn add_artwork(
        db_context: &mut DbContext,
        show_id: Uuid,
        episode_id: Option<Uuid>,
        kind: ArtworkKind,
    ) -> Uuid {
        db_context
            .artwork()
            .create()
            .show_id(Some(show_id))
            .episode_id(episode_id)
            .kind(kind)
            .path(format!("/media/TV/Severance/{kind:?}"))
            .modified_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    fn widths(image: Option<&Image>) -> Option<(Uuid, Vec<i64>)> {
        image.map(|x| (x.id, x.urls.iter().map(|x| x.width).collect()))
    }

    async fn add_episode_with_file(
        db_context: &mut DbContext,
        numbers: &[i64],
    ) -> (Vec<Uuid>, Uuid) {
        let severance = show(db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(db_context, severance.id, 1).await;
        let media_file = db_context
            .media_files()
            .create()
            .path("/media/TV/Severance/Season 01/Severance - S01E02-E03.mkv")
            .duration_ms(Some(2_829_999))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let mut ids = Vec::new();
        for &number in numbers {
            let episode = episode(db_context, season_id)
                .number(number)
                .media_file_id(Some(media_file.id))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            ids.push(episode.id);
        }
        (ids, media_file.id)
    }

    async fn add_audio(
        db_context: &mut DbContext,
        media_file_id: Uuid,
        index: i64,
        language: Option<&str>,
    ) {
        let media_file = db_context
            .media_files()
            .find(media_file_id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        media_file
            .audio_streams()
            .create()
            .index(index)
            .codec("eac3")
            .language(language.map(str::to_owned))
            .channels(6)
            .is_default(index == 1)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn add_subtitle_stream(
        db_context: &mut DbContext,
        media_file_id: Uuid,
        index: i64,
        language: &str,
        (text, hearing_impaired, forced): (bool, bool, bool),
    ) {
        let media_file = db_context
            .media_files()
            .find(media_file_id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        media_file
            .subtitle_streams()
            .create()
            .index(index)
            .codec(if text { "subrip" } else { "hdmv_pgs_subtitle" })
            .language(Some(language.to_owned()))
            .is_default(false)
            .forced(forced)
            .hearing_impaired(hearing_impaired)
            .text(text)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    fn subtitle(language: &str, hearing_impaired: bool, forced: bool) -> domain::SubtitleLanguage {
        domain::SubtitleLanguage {
            language: language.to_owned(),
            hearing_impaired,
            forced,
        }
    }

    #[tokio::test]
    async fn both_episodes_in_one_file_carry_the_file_s_languages_and_runtime() {
        let mut db_context = mock_db_context().await;
        let (episodes, media_file) = add_episode_with_file(&mut db_context, &[2, 3]).await;
        add_audio(&mut db_context, media_file, 1, Some("eng")).await;
        add_subtitle_stream(&mut db_context, media_file, 2, "eng", (true, false, false)).await;

        for (episode_id, number) in episodes.into_iter().zip([2, 3]) {
            let page = get_episode(&db_context, episode_id).await;

            assert_eq!(page.episode_number, number);
            assert_eq!(page.runtime_minutes, Some(47));
            assert_eq!(page.audio_languages, ["eng"]);
            assert_eq!(page.subtitle_languages, [subtitle("eng", false, false)]);
        }
    }

    #[tokio::test]
    async fn an_unknown_episode_or_one_not_in_the_library_is_not_found() {
        let mut db_context = mock_db_context().await;
        let severance = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(&mut db_context, severance.id, 1).await;
        let out_of_library = episode(&db_context, season_id)
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        for episode_id in [out_of_library.id, Uuid::new_v4()] {
            let output = get(&db_context, episode_id).await;

            assert!(matches!(output, Output::NotFound), "{output:?}");
        }
    }

    #[tokio::test]
    async fn an_episode_page_carries_its_metadata_thumb_and_show() {
        let mut db_context = mock_db_context().await;
        let severance = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(&mut db_context, severance.id, 1).await;
        let media_file = db_context
            .media_files()
            .create()
            .path("/media/TV/Severance/Season 01/Severance - S01E02.mkv")
            .duration_ms(Some(3_420_000))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let half_loop = episode(&db_context, season_id)
            .number(2)
            .title("Half Loop")
            .air_date(Some(date(2022, 2, 18)))
            .rating(Some(8.1))
            .overview(Some("Mark takes Helly to the break room.".to_owned()))
            .media_file_id(Some(media_file.id))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let thumb = add_artwork(
            &mut db_context,
            severance.id,
            Some(half_loop.id),
            ArtworkKind::Thumb,
        )
        .await;
        add_artwork(&mut db_context, severance.id, None, ArtworkKind::Fanart).await;
        let logo = add_artwork(&mut db_context, severance.id, None, ArtworkKind::Clearlogo).await;

        let page = get_episode(&db_context, half_loop.id).await;

        assert_eq!(page.id, half_loop.id);
        assert_eq!(page.title, "Half Loop");
        assert_eq!((page.season_number, page.episode_number), (1, 2));
        assert_eq!(page.air_date, Some(date(2022, 2, 18)));
        assert_eq!(page.runtime_minutes, Some(57));
        assert_eq!(page.rating, Some(8.1));
        assert_eq!(
            page.plot.as_deref(),
            Some("Mark takes Helly to the break room.")
        );
        assert_eq!(widths(page.thumb.as_ref()), Some((thumb, vec![480, 960])));
        assert_eq!(
            (page.show.id, page.show.title.as_str()),
            (severance.id, "Severance")
        );
        assert_eq!(
            widths(page.show.logo.as_ref()),
            Some((logo, vec![300, 600]))
        );
    }

    #[tokio::test]
    async fn each_user_sees_their_own_resume_position_progress_minutes_left_and_watched_state() {
        let mut db_context = mock_db_context().await;
        let (episodes, media_file) = add_episode_with_file(&mut db_context, &[2]).await;
        let (noor, ravi, mina) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        stop_at(
            &db_context,
            noor,
            media_file,
            SignedDuration::from_secs(1415),
            "2026-11-02T20:00:00Z",
        )
        .await;
        watch_to_the_end(&db_context, ravi, media_file, "2026-11-02T20:00:00Z").await;

        let mut views = Vec::new();
        for user_id in [noor, ravi, mina] {
            let Output::Episode(page) = call(dependencies(&db_context), user_id, episodes[0])
                .await
                .unwrap()
            else {
                panic!("the episode is not found");
            };
            let percent = page.progress.map(|x| (x * 100.0).round());
            views.push((
                page.resume_position_seconds,
                percent,
                page.minutes_left,
                page.watched,
            ));
        }

        assert_eq!(
            views,
            [
                (Some(1415), Some(50.0), Some(24), false),
                (None, None, None, true),
                (None, None, None, false),
            ]
        );
    }
}
