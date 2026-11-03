import type { MoviePage } from "@selfless/api-client";
import type { ReactNode } from "react";
import { type DetailFact, DetailHeader, type DetailItem } from "./DetailHeader";
import { runtimeLabel } from "./runtimeLabel";

export function MovieHeader({ movie, children }: { movie: MoviePage; children?: ReactNode }) {
  const details: DetailItem[] = [
    { text: String(movie.year) },
    ...(movie.certification ? [{ text: movie.certification, outlined: true }] : []),
    ...(movie.runtimeMinutes ? [{ text: runtimeLabel(movie.runtimeMinutes) }] : []),
    ...(movie.genres.length > 0 ? [{ text: movie.genres.join(" · ") }] : []),
    ...(movie.rating != null ? [{ text: `★ ${movie.rating.toFixed(1)}` }] : []),
  ];
  const facts: DetailFact[] = [
    ...(movie.directors.length > 0
      ? [{ label: movie.directors.length === 1 ? "Director" : "Directors", value: movie.directors.join(", ") }]
      : []),
    ...(movie.cast.length > 0
      ? [{ label: "Cast", value: movie.cast.map((x) => (x.role ? `${x.name} (${x.role})` : x.name)).join(", ") }]
      : []),
    ...(movie.studio && movie.studio !== movie.title ? [{ label: "Studio", value: movie.studio }] : []),
    ...(movie.originalTitle && movie.originalTitle !== movie.title
      ? [{ label: "Original title", value: movie.originalTitle }]
      : []),
  ];
  return (
    <DetailHeader
      kicker="Movie"
      title={movie.title}
      backdrop={movie.images.fanart}
      logo={movie.images.logo}
      details={details}
      plot={movie.plot}
      facts={facts}
      resume={movie}
    >
      {children}
    </DetailHeader>
  );
}
