import type { FeaturedTitle } from "@selfless/api-client";

export function playHref(target: FeaturedTitle | { episodeId: string }) {
  if ("episodeId" in target) {
    return `/play/episodes/${target.episodeId}`;
  }
  const { tile, playEpisodeId } = target;
  return playEpisodeId ? `/play/episodes/${playEpisodeId}` : `/play/movies/${tile.id}`;
}
