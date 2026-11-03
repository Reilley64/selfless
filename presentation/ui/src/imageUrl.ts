import { type Artwork, serverUrl } from "@selfless/api-client";

export function imageUrl(artwork: Artwork, pixelWidth: number) {
  const urls = [...artwork.urls].sort((a, b) => a.width - b.width);
  const url = urls.find(({ width }) => width >= pixelWidth) ?? urls[urls.length - 1];
  return serverUrl(url.url);
}
