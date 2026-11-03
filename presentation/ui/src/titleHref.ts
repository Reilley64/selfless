import type { FeaturedTitle, Tile } from "@selfless/api-client";

export function titleHref(tile: Tile) {
  switch (tile.kind) {
    case "movie":
      return `/movies/${tile.id}`;
    case "show":
      return `/shows/${tile.id}`;
    case "episode":
      return `/episodes/${tile.id}`;
  }
}

export function playHref({ tile, playEpisodeId }: FeaturedTitle) {
  return playEpisodeId ? `/play/episodes/${playEpisodeId}` : `/play/movies/${tile.id}`;
}
