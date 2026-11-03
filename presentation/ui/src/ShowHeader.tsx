import type { ShowPage, ShowPlay } from "@selfless/api-client";
import type { ReactNode } from "react";
import { type DetailFact, DetailHeader, type DetailItem } from "./DetailHeader";
import { episodeLabel } from "./episodeLabel";

const verbs = { play: "Play", resume: "Resume" } as const;

export function showPlayLabel({ verb, season, episode }: ShowPlay) {
  return `${verbs[verb]} ${episodeLabel(season.number, episode.number)}`;
}

export function ShowHeader({ show, children }: { show: ShowPage; children?: ReactNode }) {
  const details: DetailItem[] = [
    ...(show.year != null ? [{ text: String(show.year) }] : []),
    ...(show.certification ? [{ text: show.certification, outlined: true }] : []),
    ...(show.seasonCount > 0 ? [{ text: `${show.seasonCount} ${show.seasonCount === 1 ? "Season" : "Seasons"}` }] : []),
    ...(show.genres.length > 0 ? [{ text: show.genres.join(" · ") }] : []),
    ...(show.rating != null ? [{ text: `★ ${show.rating.toFixed(1)}` }] : []),
  ];
  const facts: DetailFact[] = [
    ...(show.network ? [{ label: "Network", value: show.network }] : []),
    ...(show.status ? [{ label: "Status", value: show.status }] : []),
    ...(show.cast.length > 0
      ? [{ label: "Cast", value: show.cast.map((x) => (x.role ? `${x.name} (${x.role})` : x.name)).join(", ") }]
      : []),
  ];
  return (
    <DetailHeader
      kicker="Show"
      title={show.title}
      backdrop={show.images.fanart}
      logo={show.images.logo}
      details={details}
      plot={show.plot}
      facts={facts}
    >
      {children}
    </DetailHeader>
  );
}
