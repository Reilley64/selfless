mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::EpisodeWatchState;
use domain::MovieContinueWatchingRemoval;
use domain::MovieWatchState;
use domain::Playback;
use domain::PlaybackReason;
use domain::PlaybackSettings;
use domain::Session;
use domain::ShowContinueWatchingRemoval;
use domain::User;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        end_hls_output,
    } = dependencies.into();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let open = transaction
        .playbacks()
        .filter(
            Playback::fields()
                .user_id()
                .eq(id)
                .and(Playback::open_filter()),
        )
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .users()
        .filter(User::fields().id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .sessions()
        .filter(Session::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let playbacks = transaction
        .playbacks()
        .filter(Playback::fields().user_id().eq(id))
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    transaction
        .playback_reasons()
        .filter(PlaybackReason::fields().playback_id().in_list(playbacks))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .playbacks()
        .filter(Playback::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .playback_settings()
        .filter(PlaybackSettings::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .movie_watch_states()
        .filter(MovieWatchState::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .episode_watch_states()
        .filter(EpisodeWatchState::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .movie_continue_watching_removals()
        .filter(MovieContinueWatchingRemoval::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .show_continue_watching_removals()
        .filter(ShowContinueWatchingRemoval::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    for playback in open {
        end_hls_output.call(playback.id).await;
    }
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::EndedHlsOutputs;
    use crate::test::add_playback_reason;
    use crate::test::add_season;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::playback;
    use crate::test::playback_reason_owners;
    use crate::test::show;
    use crate::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use domain::Password;
    use jiff::Timestamp;

    const PASSWORD: &str = "a long passphrase";

    async fn create_user(db_context: &DbContext, name: &str) -> Uuid {
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            EmailAddress::new(format!("{name}@example.com")).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

    async fn add_session(db_context: &mut DbContext, user_id: Uuid, device_name: &str) {
        db_context
            .sessions()
            .create()
            .user_id(user_id)
            .client_id(Uuid::new_v4())
            .client_kind(ClientKind::Web)
            .device_name(device_name)
            .token_hash(device_name.as_bytes().to_vec())
            .signed_in_at(Timestamp::UNIX_EPOCH)
            .last_used_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deleting_a_user_ends_their_sessions() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        add_session(&mut db_context, noor, "Noor's laptop").await;
        add_session(&mut db_context, noor, "Noor's phone").await;
        add_session(&mut db_context, ravi, "Ravi's laptop").await;

        call(
            Dependencies {
                db_context: db_context.clone(),
                end_hls_output: EndedHlsOutputs::default().port(),
            },
            noor,
        )
        .await
        .unwrap();

        let users = db_context
            .users()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect::<Vec<_>>();
        let sessions = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.device_name)
            .collect::<Vec<_>>();
        assert_eq!(users, [ravi]);
        assert_eq!(sessions, ["Ravi's laptop"]);
    }

    #[tokio::test]
    async fn deleting_a_user_deletes_their_playback_settings() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        for user_id in [noor, ravi] {
            db_context
                .playback_settings()
                .create()
                .user_id(user_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        call(
            Dependencies {
                db_context: db_context.clone(),
                end_hls_output: EndedHlsOutputs::default().port(),
            },
            noor,
        )
        .await
        .unwrap();

        let settings = db_context
            .playback_settings()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        assert_eq!(settings, [ravi]);
    }

    #[tokio::test]
    async fn deleting_a_user_deletes_their_playbacks_and_ends_their_hls_outputs() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        let mut ids = Vec::new();
        for user_id in [noor, ravi] {
            let id = playback(&db_context, Timestamp::UNIX_EPOCH)
                .user_id(user_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            add_playback_reason(&mut db_context, id).await;
            ids.push(id);
        }
        let ended = EndedHlsOutputs::default();

        call(
            Dependencies {
                db_context: db_context.clone(),
                end_hls_output: ended.port(),
            },
            noor,
        )
        .await
        .unwrap();

        let playbacks = db_context
            .playbacks()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        assert_eq!(playbacks, [ravi]);
        assert_eq!(playback_reason_owners(&mut db_context).await, [ids[1]]);
        assert_eq!(ended.playback_ids(), [ids[0]]);
    }

    #[tokio::test]
    async fn deleting_a_user_deletes_their_watch_state() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        let movie_id = movie(&db_context, "Arrival")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let severance = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let season_id = add_season(&mut db_context, severance.id, 1).await;
        let episode_id = episode(&db_context, season_id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        for user_id in [noor, ravi] {
            MovieWatchState::upsert_by_user_id_and_movie_id(user_id, movie_id)
                .resume_position_seconds(Some(1200))
                .watched(false)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            EpisodeWatchState::upsert_by_user_id_and_episode_id(user_id, episode_id)
                .watched(true)
                .watched_at(Some(Timestamp::UNIX_EPOCH))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        call(
            Dependencies {
                db_context: db_context.clone(),
                end_hls_output: EndedHlsOutputs::default().port(),
            },
            noor,
        )
        .await
        .unwrap();

        let movie_watch_states = db_context
            .movie_watch_states()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        let episode_watch_states = db_context
            .episode_watch_states()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        assert_eq!(movie_watch_states, [ravi]);
        assert_eq!(episode_watch_states, [ravi]);
    }

    #[tokio::test]
    async fn deleting_a_user_deletes_their_continue_watching_removals() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        let movie_id = movie(&db_context, "Arrival")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let show_id = show(&db_context, "Severance")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        for user_id in [noor, ravi] {
            MovieContinueWatchingRemoval::upsert_by_user_id_and_movie_id(user_id, movie_id)
                .removed_at(Timestamp::UNIX_EPOCH)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            ShowContinueWatchingRemoval::upsert_by_user_id_and_show_id(user_id, show_id)
                .removed_at(Timestamp::UNIX_EPOCH)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        call(
            Dependencies {
                db_context: db_context.clone(),
                end_hls_output: EndedHlsOutputs::default().port(),
            },
            noor,
        )
        .await
        .unwrap();

        let movie_removals = db_context
            .movie_continue_watching_removals()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        let show_removals = db_context
            .show_continue_watching_removals()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.user_id)
            .collect::<Vec<_>>();
        assert_eq!(movie_removals, [ravi]);
        assert_eq!(show_removals, [ravi]);
    }
}
