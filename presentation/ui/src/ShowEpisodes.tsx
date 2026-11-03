import type { ShowPage, ShowSeason } from "@selfless/api-client";
import { router, useLocalSearchParams } from "expo-router";
import { useState } from "react";
import { Platform } from "react-native";
import { Pressable, Text, View } from "react-native-css/components";
import { EpisodeList } from "./EpisodeList";
import { FocusGuide, type FocusGuideInstance } from "./FocusGuide";
import { Select } from "./GridSelectors";

export function useShowSeason(show: ShowPage): ShowSeason | undefined {
  const { season } = useLocalSearchParams<{ season?: string }>();
  return (
    show.seasons.find((x) => String(x.number) === season) ??
    show.seasons.find((x) => x.id === show.play.season.id) ??
    show.seasons[0]
  );
}

function SeasonColumn({ seasons, current }: { seasons: ShowSeason[]; current: ShowSeason }) {
  const [nodes, setNodes] = useState<Record<string, FocusGuideInstance>>({});
  const currentNode = nodes[current.id];
  return (
    <FocusGuide
      autoFocus
      destinations={currentNode ? [currentNode] : []}
      role="tablist"
      className="w-[300px] gap-2"
    >
      {seasons.map((season) => {
        const selected = season.id === current.id;
        return (
          <Pressable
            key={season.id}
            ref={(node) => {
              const guide = node as unknown as FocusGuideInstance | null;
              if (guide) {
                setNodes((x) => (x[season.id] === guide ? x : { ...x, [season.id]: guide }));
              }
            }}
            role="tab"
            aria-label={season.name}
            aria-selected={selected}
            className={`group rounded-medium px-6 py-4 transition-transform focus:scale-104 focus:bg-button-focus ${selected ? "border-accent border-l-[5px]" : ""}`}
            onFocus={() => router.setParams({ season: String(season.number) })}
          >
            <Text
              className={`font-sans font-semibold text-tv-row group-focus:text-text-inverse ${selected ? "text-text-primary" : "text-text-muted"}`}
            >
              {season.name}
            </Text>
            <Text
              className={`font-sans text-tv-caption group-focus:text-text-inverse ${selected ? "text-text-secondary" : "text-text-muted"}`}
            >
              {`${season.episodeCount} ${season.episodeCount === 1 ? "episode" : "episodes"}`}
            </Text>
          </Pressable>
        );
      })}
    </FocusGuide>
  );
}

export function ShowEpisodes({ show }: { show: ShowPage }) {
  const season = useShowSeason(show);
  if (!season) {
    return null;
  }
  if (Platform.isTV) {
    return (
      <View className="gap-6 px-tv-inset pt-6">
        <Text role="heading" className="font-extrabold font-sans text-text-primary text-tv-heading">
          Episodes
        </Text>
        <View className="flex-row items-start gap-[50px]">
          <SeasonColumn seasons={show.seasons} current={season} />
          <FocusGuide key={season.id} autoFocus className="flex-1">
            <EpisodeList seasonId={season.id} />
          </FocusGuide>
        </View>
      </View>
    );
  }
  return (
    <View className="z-50 gap-5 pt-6">
      <View className="z-50 flex-row items-center gap-5 px-web-inset">
        <Text role="heading" className="font-extrabold font-sans text-text-primary text-web-heading">
          Episodes
        </Text>
        <Select
          label="Season"
          current={season.name}
          options={show.seasons.map((x) => ({ value: String(x.number), label: x.name, count: x.episodeCount }))}
          onChange={(value) => router.setParams({ season: value })}
        />
      </View>
      <EpisodeList seasonId={season.id} />
    </View>
  );
}
