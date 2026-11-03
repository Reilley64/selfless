import type { Tile as TileData } from "@selfless/api-client";
import { Platform } from "react-native";
import { Pressable, Text } from "react-native-css/components";

export function Tile({ tile, onPress }: { tile: TileData; onPress?: () => void }) {
  return (
    <Pressable
      className="aspect-video w-full justify-end overflow-hidden rounded-medium bg-surface p-3"
      aria-label={tile.title}
      onPress={onPress}
    >
      <Text
        className={`font-sans font-extrabold text-text-primary ${Platform.isTV ? "text-tv-row" : "text-web-row"}`}
        numberOfLines={2}
      >
        {tile.title}
      </Text>
    </Pressable>
  );
}
