import type { Tile as TileData } from "@selfless/api-client";
import { Platform } from "react-native";
import { ScrollView, Text, View } from "react-native-css/components";
import { Tile } from "./Tile";

export function TileRow({
  title,
  tiles,
  onPressTile,
}: {
  title: string;
  tiles: TileData[];
  onPressTile?: (tile: TileData) => void;
}) {
  const tv = Platform.isTV;
  return (
    <View className="gap-2">
      <Text
        className={`font-sans font-bold text-text-secondary ${tv ? "px-tv-inset text-tv-row" : "px-web-inset text-web-row"}`}
      >
        {title}
      </Text>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerClassName={tv ? "gap-tile-tv-gap px-tv-inset" : "gap-tile-web-gap px-web-inset"}
      >
        {tiles.map((tile) => (
          <View key={tile.id} className={tv ? "w-tile-tv-width" : "w-tile-web-width"}>
            <Tile tile={tile} onPress={onPressTile && (() => onPressTile(tile))} />
          </View>
        ))}
      </ScrollView>
    </View>
  );
}
