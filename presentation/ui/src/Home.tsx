import { type HomeRow, type HomeRowKind, useGetHome } from "@selfless/api-client";
import { ScrollView, Text, View } from "react-native-css/components";
import { FeaturedTitle } from "./FeaturedTitle";
import { serverUnreachable } from "./messages";
import { TileRow } from "./TileRow";

const rowTitles: Record<Exclude<HomeRowKind, "genre">, string> = {
  continueWatching: "Continue watching",
  recentlyAdded: "Recently added",
  popularNow: "Popular now",
};

export function rowTitle(row: HomeRow) {
  return row.kind === "genre" ? (row.genre ?? "") : rowTitles[row.kind];
}

export function Home() {
  const { data, isError } = useGetHome();
  if (isError) {
    return <Text className="p-web-inset font-sans text-web-body text-text-muted">{serverUnreachable}</Text>;
  }
  if (data?.status !== 200) {
    return null;
  }
  const { featured, rows } = data.data;
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="pb-16">
      {featured && <FeaturedTitle featured={featured} />}
      <View className={featured ? "-mt-[150px]" : "pt-8"}>
        {rows
          .filter((row) => row.tiles.length > 0)
          .map((row) => (
            <TileRow key={`${row.kind} ${row.genre}`} title={rowTitle(row)} tiles={row.tiles} />
          ))}
      </View>
    </ScrollView>
  );
}
