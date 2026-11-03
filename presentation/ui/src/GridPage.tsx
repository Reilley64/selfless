import { type TilePage, useListMoviesInfinite, useListShowsInfinite } from "@selfless/api-client";
import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useRef } from "react";
import { type View as HostView, Platform, useWindowDimensions } from "react-native";
import { Text, View } from "react-native-css/components";
import { FocusGuide, type FocusGuideInstance } from "./FocusGuide";
import { GridSelectors, useGridQuery } from "./GridSelectors";
import { serverUnreachable } from "./messages";
import { TileGrid } from "./TileGrid";
import { tilePath } from "./tilePath";

type TilePageResponse = { status: 200; data: TilePage } | { status: 400 | 401 | 500; data: unknown };

const TV_COLUMNS = 5;
const MIN_WEB_TILE_WIDTH = 240;
const WEB_GAP = 16;
const WEB_INSET = 48;

const nextCursor = (page: TilePageResponse) => (page.status === 200 ? (page.data.nextCursor ?? undefined) : undefined);

export function MoviesGrid() {
  const query = useListMoviesInfinite(useGridQuery(), {
    query: { initialPageParam: undefined, getNextPageParam: nextCursor },
  });
  return <GridPage title="Movies" kind="movie" query={query} />;
}

export function ShowsGrid() {
  const query = useListShowsInfinite(useGridQuery(), {
    query: { initialPageParam: undefined, getNextPageParam: nextCursor },
  });
  return <GridPage title="Shows" kind="show" query={query} />;
}

function GridPage({
  title,
  kind,
  query,
}: {
  title: string;
  kind: "movie" | "show";
  query: UseInfiniteQueryResult<InfiniteData<TilePageResponse, string | undefined>, unknown>;
}) {
  const router = useRouter();
  const { genre, sort } = useGridQuery();
  const { width } = useWindowDimensions();
  const focusOnLoad = useRef(true);
  const tiles = query.data?.pages.flatMap((page) => (page.status === 200 ? page.data.tiles : [])) ?? [];
  const focusFirstTile = (tile: HostView | null) => {
    if (tile && focusOnLoad.current) {
      focusOnLoad.current = false;
      (tile as unknown as FocusGuideInstance).requestTVFocus();
    }
  };
  const loadMore = () => {
    if (query.hasNextPage && !query.isFetchingNextPage) {
      void query.fetchNextPage();
    }
  };
  const error = query.isError && (
    <Text className={`font-sans text-accent ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}>
      {serverUnreachable}
    </Text>
  );

  if (Platform.isTV) {
    return (
      <View className="flex-1 bg-background">
        <View className="z-10 gap-6 pt-14 pr-[60px] pb-4 pl-tv-inset">
          <View className="flex-row items-center gap-10">
            <Text role="heading" className="font-sans font-extrabold text-tv-title text-text-primary">
              {title}
            </Text>
            <FocusGuide autoFocus className="flex-1 flex-row">
              <GridSelectors kind={kind} />
            </FocusGuide>
          </View>
          {error}
        </View>
        <FocusGuide autoFocus className="flex-1 pr-[50px] pl-[140px]">
          <TileGrid
            key={`${genre} ${sort}`}
            tiles={tiles}
            numColumns={TV_COLUMNS}
            onEndReached={loadMore}
            onPressTile={(tile) => router.push(tilePath(tile))}
            header={<View className="h-4" />}
            firstTileRef={focusFirstTile}
          />
        </FocusGuide>
      </View>
    );
  }

  const numColumns = Math.max(1, Math.floor((width - 2 * WEB_INSET + WEB_GAP) / (MIN_WEB_TILE_WIDTH + WEB_GAP)));
  return (
    <View className="flex-1 bg-background px-10">
      <View className="z-10 gap-4 px-2 pt-8 pb-5">
        <View className="flex-row items-center gap-6">
          <Text className="font-sans font-extrabold text-web-heading text-text-primary">{title}</Text>
          <GridSelectors kind={kind} />
        </View>
        {error}
      </View>
      <TileGrid
        key={`${numColumns} ${genre} ${sort}`}
        tiles={tiles}
        numColumns={numColumns}
        onEndReached={loadMore}
        onPressTile={(tile) => router.push(tilePath(tile))}
      />
    </View>
  );
}
