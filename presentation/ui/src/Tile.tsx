import type { Artwork, Tile as TileData } from "@selfless/api-client";
import { useState } from "react";
import { type ImageResizeMode, PixelRatio, Platform } from "react-native";
import { Image, Pressable, Text, View } from "react-native-css/components";
import { imageUrl } from "./imageUrl";

export function ArtworkImage({
  artwork,
  className,
  resizeMode,
  testID,
}: {
  artwork: Artwork;
  className: string;
  resizeMode: ImageResizeMode;
  testID: string;
}) {
  const [width, setWidth] = useState(0);
  return (
    <Image
      testID={testID}
      className={className}
      resizeMode={resizeMode}
      onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
      source={width > 0 ? { uri: imageUrl(artwork, PixelRatio.getPixelSizeForLayoutSize(width)) } : undefined}
    />
  );
}

export function Tile({ tile, onPress }: { tile: TileData; onPress?: () => void }) {
  const { fanart, logo } = tile.images;
  return (
    <Pressable
      className="aspect-video w-full justify-end overflow-hidden rounded-medium bg-surface p-3"
      aria-label={tile.title}
      onPress={onPress}
    >
      {fanart && (
        <ArtworkImage artwork={fanart} testID="tile-fanart" className="absolute inset-0" resizeMode="cover" />
      )}
      {fanart && logo ? (
        <View className="h-2/5 w-3/5">
          <ArtworkImage artwork={logo} testID="tile-logo" className="h-full w-full" resizeMode="contain" />
        </View>
      ) : (
        <Text
          className={`self-start font-sans font-extrabold text-text-primary ${fanart ? "rounded-medium bg-scrim px-2 py-1" : ""} ${Platform.isTV ? "text-tv-row" : "text-web-row"}`}
          numberOfLines={2}
        >
          {tile.title}
        </Text>
      )}
    </Pressable>
  );
}
