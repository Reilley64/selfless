import type { Tile } from "@selfless/api-client";
import { Text, View } from "react-native-css/components";
import { episodeLabel } from "./episodeLabel";
import { ArtworkImage } from "./Tile";
import { TitleMeta } from "./TitleMeta";

export function FocusedDetails({ tile }: { tile: Tile }) {
  const backdrop = tile.images.fanart ?? tile.images.thumb;
  const { episode, progress, runtimeMinutes } = tile;
  return (
    <View className="absolute inset-x-0 top-0 h-[540px] bg-background">
      {backdrop && (
        <ArtworkImage
          key={tile.id}
          artwork={backdrop}
          testID="focused-backdrop"
          className="absolute top-0 right-0 h-full w-[1300px]"
          resizeMode="cover"
        />
      )}
      <View className="absolute inset-0 bg-linear-to-r from-background from-30% via-background/55 via-50% to-transparent to-75%" />
      <View className="absolute inset-0 bg-linear-to-t from-background to-transparent to-45%" />
      <View className="absolute top-16 left-tv-inset w-[780px] gap-4">
        <Text className="font-sans font-extrabold text-tv-label tracking-[0.22em] text-accent uppercase">
          {tile.kind === "movie" ? "Movie" : "Show"}
        </Text>
        <Text
          role="heading"
          className="font-sans font-extrabold text-tv-title text-text-primary leading-title"
          numberOfLines={1}
        >
          {episode ? episode.show.title : tile.title}
        </Text>
        <TitleMeta tile={tile} />
        {episode && (
          <Text className="font-sans text-tv-body text-text-primary" numberOfLines={1}>
            <Text className="font-bold">{episodeLabel(episode.seasonNumber, episode.episodeNumber)}</Text> {tile.title}
          </Text>
        )}
        {progress != null && (
          <View className="flex-row items-center gap-4">
            <View className="h-1.5 w-[260px] overflow-hidden rounded-pill bg-progress-track">
              <View className="h-full bg-progress-fill" style={{ width: `${progress * 100}%` }} />
            </View>
            {runtimeMinutes != null && (
              <Text className="font-sans text-tv-label text-text-secondary">
                {`${Math.round(runtimeMinutes * (1 - progress))} min left`}
              </Text>
            )}
          </View>
        )}
        {tile.plot && (
          <Text className="font-sans text-tv-body text-text-secondary leading-body" numberOfLines={3}>
            {tile.plot}
          </Text>
        )}
      </View>
    </View>
  );
}
