import type { ShowPage, ShowSeason } from "@selfless/api-client";
import { router, useLocalSearchParams } from "expo-router";
import { Text, View } from "react-native-css/components";
import { EpisodeList } from "./EpisodeList";
import { Select } from "./GridSelectors";

export function useShowSeason(show: ShowPage): ShowSeason | undefined {
  const { season } = useLocalSearchParams<{ season?: string }>();
  return (
    show.seasons.find((x) => String(x.number) === season) ??
    show.seasons.find((x) => x.id === show.play.season.id) ??
    show.seasons[0]
  );
}

export function ShowEpisodes({ show }: { show: ShowPage }) {
  const season = useShowSeason(show);
  if (!season) {
    return null;
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
