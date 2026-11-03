import type { Tile as TileData } from "@selfless/api-client";
import { FlashList } from "@shopify/flash-list";
import type { ReactElement, Ref } from "react";
import { type View as HostView, Platform } from "react-native";
import { View } from "react-native-css/components";
import { Tile } from "./Tile";

const FOCUS_ROWS_BEFORE_END = 2;

export function TileGrid({
  tiles,
  numColumns,
  onEndReached,
  onPressTile,
  header,
  firstTileRef,
}: {
  tiles: TileData[];
  numColumns: number;
  onEndReached: () => void;
  onPressTile?: (tile: TileData) => void;
  header?: ReactElement;
  firstTileRef?: Ref<HostView>;
}) {
  return (
    <FlashList
      data={tiles}
      numColumns={numColumns}
      keyExtractor={(tile) => tile.id}
      onEndReached={onEndReached}
      onEndReachedThreshold={0.5}
      ListHeaderComponent={header}
      renderItem={({ item, index }) => (
        <View className={Platform.isTV ? "px-[10px] pb-tile-tv-gap" : "px-2 pb-8"}>
          <Tile
            ref={index === 0 ? firstTileRef : undefined}
            tile={item}
            onPress={onPressTile && (() => onPressTile(item))}
            onFocus={() => {
              if (index >= tiles.length - FOCUS_ROWS_BEFORE_END * numColumns) {
                onEndReached();
              }
            }}
          />
        </View>
      )}
    />
  );
}
