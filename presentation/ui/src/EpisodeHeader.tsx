import type { EpisodePage, SubtitleLanguage } from "@selfless/api-client";
import type { ReactNode } from "react";
import { type DetailFact, DetailHeader, type DetailItem } from "./DetailHeader";
import { episodeLabel } from "./episodeLabel";
import { runtimeLabel } from "./runtimeLabel";

const languageNames =
  typeof Intl.DisplayNames === "function" ? new Intl.DisplayNames(["en"], { type: "language" }) : undefined;
const airDates = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeZone: "UTC" });

function subtitleLabel({ language, hearingImpaired, forced }: SubtitleLanguage) {
  const name = languageNames?.of(language) ?? language;
  return forced ? `${name} (Forced)` : hearingImpaired ? `${name} (SDH)` : name;
}

function languageFact(label: string, names: string[]): DetailFact[] {
  return names.length > 0 ? [{ label, value: [...new Set(names)].join(", ") }] : [];
}

export function EpisodeHeader({ episode, children }: { episode: EpisodePage; children?: ReactNode }) {
  const details: DetailItem[] = [
    ...(episode.airDate ? [{ text: airDates.format(new Date(episode.airDate)) }] : []),
    ...(episode.runtimeMinutes ? [{ text: runtimeLabel(episode.runtimeMinutes) }] : []),
    ...(episode.rating != null ? [{ text: `★ ${episode.rating.toFixed(1)}` }] : []),
  ];
  const facts = [
    ...languageFact(
      "Audio",
      episode.audioLanguages.map((x) => languageNames?.of(x) ?? x),
    ),
    ...languageFact("Subtitles", episode.subtitleLanguages.map(subtitleLabel)),
  ];
  return (
    <DetailHeader
      kicker="Episode"
      title={episode.show.title}
      backdrop={episode.thumb}
      logo={episode.show.logo}
      subtitle={`${episodeLabel(episode.seasonNumber, episode.episodeNumber)} · ${episode.title}`}
      details={details}
      plot={episode.plot}
      facts={facts}
      resume={episode}
    >
      {children}
    </DetailHeader>
  );
}
