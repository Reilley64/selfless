use crate::browsing::Tile;
use crate::browsing::movie_watches;
use crate::browsing::season_counts;
use crate::browsing::title_images;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use crate::url_signer::UrlSigner;
use domain::Artwork;
use domain::Movie;
use domain::Season;
use domain::Show;
use domain::Title;
use jiff::Timestamp;
use std::collections::HashMap;
use uuid::Uuid;

pub(crate) async fn more_like_this(
    db_context: &mut DbContext,
    user_id: Uuid,
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
        .filter(Movie::in_library_filter().and(movie_genre))
        .exec(db_context.executor_mut())
        .await?;
    let shows = db_context
        .shows()
        .filter(Show::in_library_filter().and(show_genre))
        .exec(db_context.executor_mut())
        .await?;
    let titles: Vec<Title> = movies
        .iter()
        .map(Title::movie)
        .chain(shows.iter().map(|x| Title::show(x, [])))
        .collect();
    let ids = domain::more_like_this(title_id, genres, &titles);
    let seasons = db_context
        .seasons()
        .filter(
            Season::fields()
                .show_id()
                .in_list(ids.clone())
                .and(Season::in_library_filter()),
        )
        .exec(db_context.executor_mut())
        .await?;
    let season_counts = season_counts(&seasons);
    let mut movies: HashMap<Uuid, Movie> = movies
        .into_iter()
        .filter(|x| ids.contains(&x.id))
        .map(|x| (x.id, x))
        .collect();
    let mut shows: HashMap<Uuid, Show> = shows
        .into_iter()
        .filter(|x| ids.contains(&x.id))
        .map(|x| (x.id, x))
        .collect();
    let mut watches = movie_watches(db_context, user_id, movies.values()).await?;
    let mut images = title_images(
        db_context,
        |x| x.movie_id.or(x.show_id),
        Artwork::fields()
            .movie_id()
            .in_list(ids.clone())
            .or(Artwork::fields().show_id().in_list(ids.clone())),
        url_signer,
        now,
    )
    .await?;
    Ok(ids
        .into_iter()
        .filter_map(|id| {
            let images = images.remove(&id).unwrap_or_default();
            if let Some(movie) = movies.remove(&id) {
                let watch = watches.remove(&id).unwrap_or_default();
                return Some(Tile::movie(movie, images, watch));
            }
            let show = shows.remove(&id)?;
            let season_count = season_counts.get(&id).copied().unwrap_or_default();
            Some(Tile::show(show, season_count, images))
        })
        .collect())
}
