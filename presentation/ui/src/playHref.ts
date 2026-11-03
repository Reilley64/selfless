import type { FeaturedTitle } from "@selfless/api-client";

export function playHref({ tile, playEpisodeId }: FeaturedTitle) {
  return playEpisodeId ? `/play/episodes/${playEpisodeId}` : `/play/movies/${tile.id}`;
}
