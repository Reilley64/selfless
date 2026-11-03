use crate::ports::FoundFile;
use crate::ports::NfoActor;
use crate::ports::PortFuture;
use crate::ports::UnreadableNfo;
use domain::Actor;
use domain::Artwork;
use domain::ArtworkKind;
use domain::SyncProblem;
use domain::SyncProblemKind;
use jiff::Timestamp;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) enum Owner {
    Movie(Uuid),
    Show(Uuid),
}

impl Owner {
    fn movie_id(self) -> Option<Uuid> {
        match self {
            Owner::Movie(id) => Some(id),
            Owner::Show(_) => None,
        }
    }

    fn show_id(self) -> Option<Uuid> {
        match self {
            Owner::Movie(_) => None,
            Owner::Show(id) => Some(id),
        }
    }
}

pub(crate) enum Nfo<T> {
    Missing,
    Unchanged,
    Read(T, Timestamp),
    Unreadable(String),
}

impl<T> Nfo<T> {
    pub(crate) async fn read(
        found: Option<FoundFile>,
        stored_modified_at: Option<Timestamp>,
        read: impl FnOnce(&str) -> PortFuture<Result<T, UnreadableNfo>>,
    ) -> Self {
        match found {
            None => Nfo::Missing,
            Some(file) if stored_modified_at == Some(file.modified_at) => Nfo::Unchanged,
            Some(file) => match read(&file.path).await {
                Ok(nfo) => Nfo::Read(nfo, file.modified_at),
                Err(error) => {
                    warn_unreadable(&error, &file.path);
                    Nfo::Unreadable(file.path)
                }
            },
        }
    }

    pub(crate) fn unreadable_path(&self) -> Option<&str> {
        match self {
            Nfo::Unreadable(path) => Some(path),
            _ => None,
        }
    }
}

pub(crate) fn warn_unreadable(error: &UnreadableNfo, path: &str) {
    tracing::warn!(
        error = error as &dyn std::error::Error,
        file.path = path,
        "failed to read an NFO file"
    );
}

pub(crate) async fn save_actors<T>(
    db: &mut dyn toasty::Executor,
    owner: Owner,
    had_nfo: bool,
    nfo: &Nfo<T>,
    actors_of: impl FnOnce(&T) -> &[NfoActor],
) -> toasty::Result<()> {
    let actors = match nfo {
        Nfo::Unchanged => return Ok(()),
        Nfo::Read(nfo, _) => actors_of(nfo),
        Nfo::Missing | Nfo::Unreadable(_) => &[],
    };
    if had_nfo {
        Actor::filter(actor_owner(owner)).delete().exec(db).await?;
    }
    for actor in actors {
        toasty::create!(Actor {
            movie_id: owner.movie_id(),
            show_id: owner.show_id(),
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

#[derive(PartialEq)]
pub(crate) struct FoundArtwork {
    pub(crate) season_id: Option<Uuid>,
    pub(crate) episode_id: Option<Uuid>,
    pub(crate) kind: ArtworkKind,
    pub(crate) file: FoundFile,
}

pub(crate) fn item_artwork(
    poster: Option<FoundFile>,
    fanart: Option<FoundFile>,
    clearlogo: Option<FoundFile>,
) -> Vec<FoundArtwork> {
    [
        (ArtworkKind::Poster, poster),
        (ArtworkKind::Fanart, fanart),
        (ArtworkKind::Clearlogo, clearlogo),
    ]
    .into_iter()
    .filter_map(|(kind, file)| {
        Some(FoundArtwork {
            season_id: None,
            episode_id: None,
            kind,
            file: file?,
        })
    })
    .collect()
}

pub(crate) async fn save_artwork(
    db: &mut dyn toasty::Executor,
    owner: Owner,
    found: Vec<FoundArtwork>,
) -> toasty::Result<bool> {
    let stored = Artwork::filter(artwork_owner(owner))
        .exec(db)
        .await?
        .into_iter()
        .map(|x| FoundArtwork {
            season_id: x.season_id,
            episode_id: x.episode_id,
            kind: x.kind,
            file: FoundFile {
                path: x.path,
                modified_at: x.modified_at,
            },
        })
        .collect::<Vec<_>>();
    if stored.len() == found.len() && found.iter().all(|x| stored.contains(x)) {
        return Ok(false);
    }
    Artwork::filter(artwork_owner(owner))
        .delete()
        .exec(db)
        .await?;
    for artwork in found {
        toasty::create!(Artwork {
            movie_id: owner.movie_id(),
            show_id: owner.show_id(),
            season_id: artwork.season_id,
            episode_id: artwork.episode_id,
            kind: artwork.kind,
            path: artwork.file.path,
            modified_at: artwork.file.modified_at,
        })
        .exec(db)
        .await?;
    }
    Ok(true)
}

pub(crate) async fn save_unreadable_nfos(
    db: &mut dyn toasty::Executor,
    owner: Owner,
    media_file_ids: Vec<Uuid>,
    found: HashSet<(Option<Uuid>, String)>,
) -> toasty::Result<()> {
    let unreadable_nfo = SyncProblem::fields()
        .kind()
        .eq(SyncProblemKind::UnreadableNfo);
    let mut stored = SyncProblem::filter(problem_owner(owner).and(unreadable_nfo.clone()))
        .exec(db)
        .await?;
    if !media_file_ids.is_empty() {
        stored.extend(
            SyncProblem::filter(
                SyncProblem::fields()
                    .media_file_id()
                    .in_list(media_file_ids)
                    .and(unreadable_nfo),
            )
            .exec(db)
            .await?,
        );
    }
    if stored
        .iter()
        .map(|x| (x.media_file_id, x.path.clone()))
        .collect::<HashSet<_>>()
        == found
    {
        return Ok(());
    }
    for problem in stored {
        problem.delete().exec(db).await?;
    }
    for (media_file_id, path) in found {
        let owner = media_file_id.is_none().then_some(owner);
        toasty::create!(SyncProblem {
            movie_id: owner.and_then(Owner::movie_id),
            show_id: owner.and_then(Owner::show_id),
            media_file_id,
            kind: SyncProblemKind::UnreadableNfo,
            path,
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

pub(crate) async fn delete(db: &mut dyn toasty::Executor, owner: Owner) -> toasty::Result<()> {
    Actor::filter(actor_owner(owner)).delete().exec(db).await?;
    Artwork::filter(artwork_owner(owner))
        .delete()
        .exec(db)
        .await?;
    SyncProblem::filter(problem_owner(owner))
        .delete()
        .exec(db)
        .await
}

fn actor_owner(owner: Owner) -> toasty::stmt::Expr<bool> {
    match owner {
        Owner::Movie(id) => Actor::fields().movie_id().eq(id),
        Owner::Show(id) => Actor::fields().show_id().eq(id),
    }
}

fn artwork_owner(owner: Owner) -> toasty::stmt::Expr<bool> {
    match owner {
        Owner::Movie(id) => Artwork::fields().movie_id().eq(id),
        Owner::Show(id) => Artwork::fields().show_id().eq(id),
    }
}

fn problem_owner(owner: Owner) -> toasty::stmt::Expr<bool> {
    match owner {
        Owner::Movie(id) => SyncProblem::fields().movie_id().eq(id),
        Owner::Show(id) => SyncProblem::fields().show_id().eq(id),
    }
}
