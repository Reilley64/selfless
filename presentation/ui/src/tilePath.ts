import type { Tile } from "@selfless/api-client";

const folders = { movie: "movies", show: "shows", episode: "episodes" } as const;

export function tilePath(tile: Tile) {
  return `/${folders[tile.kind]}/${tile.id}`;
}
