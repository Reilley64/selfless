import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import type { TilePage } from "@selfless/api-client";
import { TileGrid } from "@selfless/ui";
import { useWindowDimensions } from "react-native";
import { Text, View } from "react-native-css/components";
import { ServerUnreachable } from "./form";

type TilePageResponse = { status: 200; data: TilePage } | { status: 400 | 401 | 500; data: unknown };

const MIN_TILE_WIDTH = 240;
const GAP = 16;
const INSET = 48;

export const nextCursor = (page: TilePageResponse) =>
  page.status === 200 ? (page.data.nextCursor ?? undefined) : undefined;

export function GridPage({
  title,
  query,
}: {
  title: string;
  query: UseInfiniteQueryResult<InfiniteData<TilePageResponse, string | undefined>, unknown>;
}) {
  const { width } = useWindowDimensions();
  const numColumns = Math.max(1, Math.floor((width - 2 * INSET + GAP) / (MIN_TILE_WIDTH + GAP)));
  const tiles = query.data?.pages.flatMap((page) => (page.status === 200 ? page.data.tiles : [])) ?? [];
  const loadMore = () => {
    if (query.hasNextPage && !query.isFetchingNextPage) {
      void query.fetchNextPage();
    }
  };
  return (
    <View className="flex-1 bg-background px-10">
      <TileGrid
        key={numColumns}
        tiles={tiles}
        numColumns={numColumns}
        onEndReached={loadMore}
        header={
          <View className="gap-4 px-2 pt-8 pb-5">
            <Text className="font-sans font-extrabold text-web-heading text-text-primary">{title}</Text>
            {query.isError && <ServerUnreachable />}
          </View>
        }
      />
    </View>
  );
}
