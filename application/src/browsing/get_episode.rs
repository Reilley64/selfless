mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Episode;
pub use output::Output;
pub use output::Show;
pub use output::SubtitleLanguage;

use crate::browsing::Image;
use crate::ports::persistence::DbScope;
use domain::Artwork;
use domain::AudioStream;
use domain::MediaFile;
use domain::SubtitleFile;
use domain::SubtitleStream;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
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
    let mut runtime_minutes = None;
    let mut audio_languages = Vec::new();
    let mut subtitle_languages = Vec::new();
    if let Some(media_file_id) = episode.media_file_id {
        runtime_minutes = db_context
            .media_files()
            .filter(MediaFile::fields().id().eq(media_file_id))
            .first()
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .and_then(|x| x.runtime_minutes());
        let mut audio_streams = db_context
            .audio_streams()
            .filter(AudioStream::fields().media_file_id().eq(media_file_id))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        audio_streams.sort_by_key(|x| x.index);
        for language in audio_streams.into_iter().filter_map(|x| x.language) {
            if !audio_languages.contains(&language) {
                audio_languages.push(language);
            }
        }
        let mut subtitle_streams = db_context
            .subtitle_streams()
            .filter(
                SubtitleStream::fields()
                    .media_file_id()
                    .eq(media_file_id)
                    .and(SubtitleStream::fields().text().eq(true)),
            )
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        subtitle_streams.sort_by_key(|x| x.index);
        let mut subtitle_files = db_context
            .subtitle_files()
            .filter(SubtitleFile::fields().media_file_id().eq(media_file_id))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        subtitle_files.sort_by(|a, b| a.path.cmp(&b.path));
        let subtitles = subtitle_streams
            .into_iter()
            .map(|x| (x.language, x.hearing_impaired, x.forced))
            .chain(
                subtitle_files
                    .into_iter()
                    .map(|x| (x.language, x.hearing_impaired, x.forced)),
            )
            .filter_map(|(language, hearing_impaired, forced)| {
                Some(SubtitleLanguage {
                    language: language?,
                    hearing_impaired,
                    forced,
                })
            });
        for subtitle in subtitles {
            if !subtitle_languages.contains(&subtitle) {
                subtitle_languages.push(subtitle);
            }
        }
    }
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
        runtime_minutes,
        rating: episode.rating,
        plot: episode.overview,
        thumb,
        show: Show {
            id: show.id,
            title: show.title,
            logo,
        },
        audio_languages,
        subtitle_languages,
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
    use crate::test::url_signer;
    use domain::ArtworkKind;
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
        call(dependencies(db_context), episode_id).await.unwrap()
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
        db_context
            .audio_streams()
            .create()
            .media_file_id(media_file_id)
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
        db_context
            .subtitle_streams()
            .create()
            .media_file_id(media_file_id)
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

    async fn add_subtitle_file(
        db_context: &mut DbContext,
        media_file_id: Uuid,
        name: &str,
        language: &str,
        hearing_impaired: bool,
    ) {
        db_context
            .subtitle_files()
            .create()
            .media_file_id(media_file_id)
            .path(format!("/media/TV/Severance/Season 01/{name}"))
            .language(Some(language.to_owned()))
            .hearing_impaired(hearing_impaired)
            .forced(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    fn subtitle(language: &str, hearing_impaired: bool, forced: bool) -> SubtitleLanguage {
        SubtitleLanguage {
            language: language.to_owned(),
            hearing_impaired,
            forced,
        }
    }

    #[tokio::test]
    async fn the_languages_are_the_audio_streams_then_the_text_subtitles_in_the_file_and_the_subtitle_files_beside_it()
     {
        let mut db_context = mock_db_context().await;
        let (episodes, media_file) = add_episode_with_file(&mut db_context, &[2]).await;
        add_audio(&mut db_context, media_file, 2, Some("jpn")).await;
        add_audio(&mut db_context, media_file, 1, Some("eng")).await;
        add_audio(&mut db_context, media_file, 3, Some("eng")).await;
        add_audio(&mut db_context, media_file, 4, None).await;
        add_subtitle_stream(&mut db_context, media_file, 6, "eng", (true, true, false)).await;
        add_subtitle_stream(&mut db_context, media_file, 5, "eng", (true, false, true)).await;
        add_subtitle_file(
            &mut db_context,
            media_file,
            "Severance - S01E02-E03.fr.hi.srt",
            "fr",
            true,
        )
        .await;
        add_subtitle_file(
            &mut db_context,
            media_file,
            "Severance - S01E02-E03.en.srt",
            "en",
            false,
        )
        .await;

        let page = get_episode(&db_context, episodes[0]).await;

        assert_eq!(page.audio_languages, ["eng", "jpn"]);
        assert_eq!(
            page.subtitle_languages,
            [
                subtitle("eng", false, true),
                subtitle("eng", true, false),
                subtitle("en", false, false),
                subtitle("fr", true, false),
            ]
        );
    }

    #[tokio::test]
    async fn image_subtitles_are_left_out_of_the_languages() {
        let mut db_context = mock_db_context().await;
        let (episodes, media_file) = add_episode_with_file(&mut db_context, &[2]).await;
        add_subtitle_stream(&mut db_context, media_file, 2, "eng", (false, false, false)).await;
        add_subtitle_stream(&mut db_context, media_file, 3, "deu", (false, false, true)).await;
        add_subtitle_stream(&mut db_context, media_file, 4, "spa", (true, false, false)).await;

        let page = get_episode(&db_context, episodes[0]).await;

        assert_eq!(page.subtitle_languages, [subtitle("spa", false, false)]);
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
    async fn an_unknown_or_hidden_episode_is_not_found() {
        let mut db_context = mock_db_context().await;
        let severance = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(&mut db_context, severance.id, 1).await;
        let hidden = episode(&db_context, season_id)
            .visible(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        for episode_id in [hidden.id, Uuid::new_v4()] {
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
}
