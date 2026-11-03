use crate::ArtworkKind;
use crate::Episode;
use crate::Movie;
use crate::Season;
use crate::Show;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Artwork {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: toasty::Deferred<Option<Movie>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: toasty::Deferred<Option<Show>>,
    #[index]
    pub season_id: Option<Uuid>,
    #[belongs_to(key = season_id, references = id)]
    pub season: toasty::Deferred<Option<Season>>,
    #[index]
    pub episode_id: Option<Uuid>,
    #[belongs_to(key = episode_id, references = id)]
    pub episode: toasty::Deferred<Option<Episode>>,
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}

impl Artwork {
    pub fn listing_order(&self) -> String {
        self.path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    fn artwork(path: &str) -> Artwork {
        Artwork {
            id: Uuid::nil(),
            movie_id: None,
            movie: Deferred::default(),
            show_id: None,
            show: Deferred::default(),
            season_id: None,
            season: Deferred::default(),
            episode_id: None,
            episode: Deferred::default(),
            kind: ArtworkKind::Poster,
            path: path.to_owned(),
            modified_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn artwork_is_listed_by_path() {
        let mut artwork = [
            artwork("/media/Movies/Arrival (2016)/poster.jpg"),
            artwork("/media/Movies/Arrival (2016)/clearlogo.png"),
            artwork("/media/Movies/Arrival (2016)/fanart.jpg"),
        ];

        artwork.sort_by_cached_key(Artwork::listing_order);

        assert_eq!(
            artwork.map(|x| x.path),
            [
                "/media/Movies/Arrival (2016)/clearlogo.png",
                "/media/Movies/Arrival (2016)/fanart.jpg",
                "/media/Movies/Arrival (2016)/poster.jpg",
            ]
        );
    }
}
