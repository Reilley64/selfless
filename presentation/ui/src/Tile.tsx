import type { Artwork, Tile as TileData } from "@selfless/api-client";
import { type Ref, useState } from "react";
import { type ImageResizeMode, PixelRatio, Platform, type View as HostView } from "react-native";
import { Image, Pressable, Text, View } from "react-native-css/components";
import { episodeLabel } from "./episodeLabel";
import { imageUrl } from "./imageUrl";
import { ProgressBar } from "./ProgressBar";
import { WatchedMark } from "./WatchedMark";

const pixelWidth = (layoutWidth: number) =>
  Platform.isTV ? layoutWidth : PixelRatio.getPixelSizeForLayoutSize(layoutWidth);

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
      source={width > 0 ? { uri: imageUrl(artwork, pixelWidth(width)) } : undefined}
    />
  );
}

export function Tile({
  ref,
  tile,
  onPress,
  onFocus,
}: {
  ref?: Ref<HostView>;
  tile: TileData;
  onPress?: () => void;
  onFocus?: () => void;
}) {
  const { episode } = tile;
  const art = tile.images.thumb ?? tile.images.fanart;
  const logo = episode ? episode.show.logo : tile.images.logo;
  const label = episode && episodeLabel(episode.seasonNumber, episode.episodeNumber);
  return (
    <Pressable
      ref={ref}
      className={`aspect-video w-full justify-end overflow-hidden rounded-medium bg-surface p-3 ${Platform.isTV ? "transition-transform focus:scale-107" : ""}`}
      aria-label={tile.title}
      onPress={onPress}
      onFocus={onFocus}
    >
      {({ focused }) => (
        <>
          {art && (
            <ArtworkImage
              artwork={art}
              testID={tile.images.thumb ? "tile-thumb" : "tile-fanart"}
              className="absolute inset-0"
              resizeMode="cover"
            />
          )}
          {art && logo ? (
            <View className="h-2/5 w-3/5">
              <ArtworkImage artwork={logo} testID="tile-logo" className="h-full w-full" resizeMode="contain" />
            </View>
          ) : (
            <Text
              className={`self-start font-sans font-extrabold text-text-primary ${art ? "rounded-medium bg-scrim px-2 py-1" : ""} ${Platform.isTV ? "text-tv-row" : "text-web-row"}`}
              numberOfLines={2}
            >
              {episode ? episode.show.title : tile.title}
            </Text>
          )}
          {label && (
            <Text
              className={`absolute top-2 left-2 rounded-small bg-scrim px-2 py-0.5 font-sans font-bold text-text-primary ${Platform.isTV ? "text-tv-caption" : "text-web-caption"}`}
            >
              {label}
            </Text>
          )}
          {tile.progress != null && <ProgressBar progress={tile.progress} className="absolute right-2 bottom-2 left-2" />}
          {tile.watched && <WatchedMark className="absolute top-2 right-2" />}
          {Platform.isTV && focused && (
            <View className="absolute inset-0 rounded-medium border-(length:--spacing-focus-ring-tv-width) border-focus-ring" />
          )}
        </>
      )}
    </Pressable>
  );
}
