import type { FeaturedTitle } from "@selfless/api-client";

export function playHref(target: FeaturedTitle | { episodeId: string } | { movieId: string }) {
  if ("episodeId" in target) {
    return `/play/episodes/${target.episodeId}`;
  }
  if ("movieId" in target) {
    return `/play/movies/${target.movieId}`;
  }
  const { tile, playEpisodeId } = target;
  return playEpisodeId ? `/play/episodes/${playEpisodeId}` : `/play/movies/${tile.id}`;
}
