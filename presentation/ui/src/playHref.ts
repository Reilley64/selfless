import type { FeaturedTitle, Tile } from "@selfless/api-client";
import type { EpisodeStart } from "./playerLogic";

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

export function nextEpisodeHref(episode: { id: string }, { autoplayed, stillWatching }: EpisodeStart) {
  return `${playHref({ episodeId: episode.id })}?position=0&autoplayed=${autoplayed}&stillWatching=${stillWatching}`;
}
