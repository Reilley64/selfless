use crate::browsing::Tile;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::browsing::visible_season;
use crate::browsing::visible_show;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use crate::url_signer::UrlSigner;
use domain::Artwork;
use domain::Movie;
use domain::Season;
use domain::Show;
use domain::sort_key;
use jiff::Timestamp;
use std::cmp::Reverse;
use uuid::Uuid;

const SIZE: usize = 20;

enum Title {
    Movie(Movie),
    Show(Show),
}

pub(crate) async fn more_like_this(
    db_context: &mut DbContext,
    genres: &[String],
    title_id: Uuid,
    url_signer: &UrlSigner,
    now: Timestamp,
) -> toasty::Result<Vec<Tile>> {
    let movie_genre = genres
        .iter()
        .map(|x| Movie::fields().genres().contains(x.clone()))
        .reduce(|a, b| a.or(b));
    let show_genre = genres
        .iter()
        .map(|x| Show::fields().genres().contains(x.clone()))
        .reduce(|a, b| a.or(b));
    let (Some(movie_genre), Some(show_genre)) = (movie_genre, show_genre) else {
        return Ok(Vec::new());
    };
    let movies = db_context
        .movies()
        .filter(Movie::fields().visible().eq(true).and(movie_genre))
        .exec(db_context.executor_mut())
        .await?;
    let shows = db_context
        .shows()
        .filter(visible_show().and(show_genre))
        .exec(db_context.executor_mut())
        .await?;
    let mut titles: Vec<_> = movies
        .into_iter()
        .map(Title::Movie)
        .chain(shows.into_iter().map(Title::Show))
        .filter(|x| x.id() != title_id)
        .map(|x| {
            let shared = x.genres().iter().filter(|x| genres.contains(x)).count();
            ((Reverse(shared), x.sort_key(), x.id()), x)
        })
        .collect();
    titles.sort_by(|a, b| a.0.cmp(&b.0));
    titles.truncate(SIZE);
    let ids: Vec<_> = titles.iter().map(|(_, x)| x.id()).collect();
    let seasons = db_context
        .seasons()
        .filter(
            Season::fields()
                .show_id()
                .in_list(ids.clone())
                .and(visible_season()),
        )
        .exec(db_context.executor_mut())
        .await?;
    let season_counts = season_counts(&seasons);
    let mut images = title_images(
        db_context,
        |x| x.movie_id.or(x.show_id),
        Artwork::fields()
            .movie_id()
            .in_list(ids.clone())
            .or(Artwork::fields().show_id().in_list(ids)),
        url_signer,
        now,
    )
    .await?;
    Ok(titles
        .into_iter()
        .map(|(_, title)| {
            let images = images.remove(&title.id()).unwrap_or_default();
            match title {
                Title::Movie(movie) => Tile::movie(movie, images),
                Title::Show(show) => {
                    let season_count = season_counts.get(&show.id).copied().unwrap_or_default();
                    Tile::show(show, season_count, images)
                }
            }
        })
        .collect())
}

impl Title {
    fn id(&self) -> Uuid {
        match self {
            Self::Movie(x) => x.id,
            Self::Show(x) => x.id,
        }
    }

    fn genres(&self) -> &[String] {
        match self {
            Self::Movie(x) => &x.genres,
            Self::Show(x) => &x.genres,
        }
    }

    fn sort_key(&self) -> (String, String) {
        match self {
            Self::Movie(x) => sort_key(x.sort_title.as_deref(), &x.title),
            Self::Show(x) => sort_key(x.sort_title.as_deref(), &x.title),
        }
    }
}
