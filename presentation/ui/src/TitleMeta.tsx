import type { Tile } from "@selfless/api-client";
import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";
import { runtimeLabel } from "./runtimeLabel";

function length(tile: Tile) {
  if (tile.runtimeMinutes) {
    return runtimeLabel(tile.runtimeMinutes);
  }
  if (tile.seasonCount != null) {
    return `${tile.seasonCount} ${tile.seasonCount === 1 ? "Season" : "Seasons"}`;
  }
  return null;
}

export function TitleMeta({ tile, small = false }: { tile: Tile; small?: boolean }) {
  const text = Platform.isTV ? "text-tv-meta" : small ? "text-web-label" : "text-web-meta";
  const parts = [tile.year?.toString(), length(tile), tile.genres.join(" · ")].filter(Boolean);
  return (
    <View className="flex-row flex-wrap items-center gap-3">
      {tile.year != null && <Text className={`font-sans text-text-secondary ${text}`}>{tile.year}</Text>}
      {tile.certification && (
        <Text className="rounded-small border border-text-muted px-1 font-sans text-web-caption text-text-secondary">
          {tile.certification}
        </Text>
      )}
      {parts.slice(tile.year != null ? 1 : 0).map((part) => (
        <Text key={part} className={`font-sans text-text-secondary ${text}`}>
          {part}
        </Text>
      ))}
    </View>
  );
}
