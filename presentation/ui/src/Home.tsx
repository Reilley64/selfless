import {
  type getHomeResponse,
  getGetHomeQueryKey,
  type HomeRow,
  type HomeRowKind,
  type Tile,
  useGetHome,
  useRemoveMovieFromContinueWatching,
  useRemoveShowFromContinueWatching,
} from "@selfless/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { Platform, type ScrollViewInstance } from "react-native";
import { ScrollView, Text, View } from "react-native-css/components";
import { FeaturedTitle } from "./FeaturedTitle";
import { FocusedDetails } from "./FocusedDetails";
import type { FocusGuideInstance } from "./FocusGuide";
import { serverUnreachable } from "./messages";
import { playHref } from "./playHref";
import { TileRow } from "./TileRow";
import { tilePath } from "./tilePath";

const rowTitles: Record<Exclude<HomeRowKind, "genre">, string> = {
  continueWatching: "Continue watching",
  recentlyAdded: "Recently added",
  popularNow: "Popular now",
};

export function rowTitle(row: HomeRow) {
  return row.kind === "genre" ? (row.genre ?? "") : rowTitles[row.kind];
}

export function Home({ autoFocus = true }: { autoFocus?: boolean }) {
  const router = useRouter();
  const { data, isError } = useGetHome();
  const [focused, setFocused] = useState<Tile | null>(null);
  const featuredGuide = useRef<FocusGuideInstance>(null);
  const rowsScroll = useRef<ScrollViewInstance>(null);
  const rowTops = useRef<number[]>([]);
  const focusOnLoad = useRef(autoFocus);
  const queryClient = useQueryClient();
  const removeMovie = useRemoveMovieFromContinueWatching();
  const removeShow = useRemoveShowFromContinueWatching();
  const loaded = data?.status === 200;
  useEffect(() => {
    if (loaded && focusOnLoad.current) {
      featuredGuide.current?.requestTVFocus();
    }
  }, [loaded]);
  if (isError) {
    return <Text className="p-web-inset font-sans text-web-body text-text-muted">{serverUnreachable}</Text>;
  }
  if (data?.status !== 200) {
    return null;
  }
  const { featured, rows } = data.data;
  const removeFromContinueWatching = (tile: Tile) => {
    queryClient.setQueryData<getHomeResponse>(getGetHomeQueryKey(), (home) =>
      home?.status === 200
        ? {
            ...home,
            data: {
              ...home.data,
              rows: home.data.rows.map((row) =>
                row.kind === "continueWatching" ? { ...row, tiles: row.tiles.filter((x) => x.id !== tile.id) } : row,
              ),
            },
          }
        : home,
    );
    const refetchHome = { onSettled: () => queryClient.invalidateQueries({ queryKey: getGetHomeQueryKey() }) };
    if (tile.episode) {
      removeShow.mutate({ showId: tile.episode.show.id }, refetchHome);
    } else {
      removeMovie.mutate({ movieId: tile.id }, refetchHome);
    }
  };
  const tileRows = rows
    .filter((row) => row.tiles.length > 0)
    .map((row, index) => (
      <View
        key={`${row.kind} ${row.genre}`}
        onLayout={(event) => {
          rowTops.current[index] = event.nativeEvent.layout.y;
        }}
      >
        <TileRow
          title={rowTitle(row)}
          tiles={row.tiles}
          onPressTile={(tile) => router.push(row.kind === "continueWatching" ? playHref(tile) : tilePath(tile))}
          menu={
            row.kind === "continueWatching"
              ? (tile) => [{ label: "Remove from Continue watching", onPress: () => removeFromContinueWatching(tile) }]
              : undefined
          }
          onFocusTile={
            Platform.isTV
              ? (tile) => {
                  setFocused(tile);
                  rowsScroll.current?.scrollTo({ y: rowTops.current[index], animated: true });
                }
              : undefined
          }
        />
      </View>
    ));
  if (Platform.isTV) {
    return (
      <View className="flex-1 bg-background">
        {featured && <FeaturedTitle featured={featured} ref={featuredGuide} onFocus={() => setFocused(null)} />}
        {focused && <FocusedDetails tile={focused} />}
        <ScrollView
          ref={rowsScroll}
          showsVerticalScrollIndicator={false}
          className={`absolute inset-x-0 bottom-0 ${focused || !featured ? "top-[540px] bg-background" : "top-[700px]"}`}
          contentContainerClassName="pb-[540px]"
        >
          {tileRows}
        </ScrollView>
      </View>
    );
  }
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="pb-16">
      {featured && <FeaturedTitle featured={featured} />}
      <View className={featured ? "-mt-[150px]" : "pt-8"}>{tileRows}</View>
    </ScrollView>
  );
}
