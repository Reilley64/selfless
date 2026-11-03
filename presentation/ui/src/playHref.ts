import type { FeaturedTitle, Tile } from "@selfless/api-client";

export function playHref(target: FeaturedTitle | Tile | { episodeId: string } | { movieId: string }) {
  if ("kind" in target) {
    return target.kind === "episode" ? `/play/episodes/${target.id}` : `/play/movies/${target.id}`;
  }
  if ("episodeId" in target) {
    return `/play/episodes/${target.episodeId}`;
  }
  if ("movieId" in target) {
    return `/play/movies/${target.movieId}`;
  }
  const { tile, playEpisodeId } = target;
  return playEpisodeId ? `/play/episodes/${playEpisodeId}` : `/play/movies/${tile.id}`;
}
