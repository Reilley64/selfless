import type { Tile as TileData } from "@selfless/api-client";
import { FlashList } from "@shopify/flash-list";
import type { ReactElement } from "react";
import { Platform } from "react-native";
import { View } from "react-native-css/components";
import { Tile } from "./Tile";

export function TileGrid({
  tiles,
  numColumns,
  onEndReached,
  onPressTile,
  header,
}: {
  tiles: TileData[];
  numColumns: number;
  onEndReached: () => void;
  onPressTile?: (tile: TileData) => void;
  header?: ReactElement;
}) {
  return (
    <FlashList
      data={tiles}
      numColumns={numColumns}
      keyExtractor={(tile) => tile.id}
      onEndReached={onEndReached}
      onEndReachedThreshold={0.5}
      ListHeaderComponent={header}
      renderItem={({ item }) => (
        <View className={Platform.isTV ? "px-[10px] pb-tile-tv-gap" : "px-2 pb-8"}>
          <Tile tile={item} onPress={onPressTile && (() => onPressTile(item))} />
        </View>
      )}
    />
  );
}
