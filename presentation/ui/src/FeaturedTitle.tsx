import type { FeaturedTitle as FeaturedTitleData } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";
import { ActionButton } from "./ActionButton";
import { ArtworkImage } from "./Tile";
import { TitleMeta } from "./TitleMeta";
import { playHref, titleHref } from "./titleHref";

export function FeaturedTitle({ featured }: { featured: FeaturedTitleData }) {
  const router = useRouter();
  const { tile } = featured;
  const { fanart, logo } = tile.images;
  return (
    <View
      className={`relative justify-end overflow-hidden ${Platform.isTV ? "h-billboard-tv-height" : "h-billboard-web-height"}`}
    >
      {fanart && (
        <ArtworkImage artwork={fanart} testID="featured-backdrop" className="absolute inset-0" resizeMode="cover" />
      )}
      <View className="absolute inset-0 bg-linear-to-r from-background/95 via-background/55 via-45% to-transparent to-70%" />
      <View className="absolute inset-0 bg-linear-to-t from-background to-transparent to-45%" />
      <View className={`w-[620px] max-w-[50%] gap-3 ${Platform.isTV ? "px-tv-inset pb-16" : "px-web-inset pb-[200px]"}`}>
        <Text
          className={`font-sans font-extrabold tracking-[0.22em] text-accent uppercase ${Platform.isTV ? "text-tv-label" : "text-web-label"}`}
        >
          {tile.kind === "movie" ? "Featured movie" : "Featured show"}
        </Text>
        {logo ? (
          <View className="h-[140px] w-[420px] max-w-full" role="heading" aria-label={tile.title}>
            <ArtworkImage artwork={logo} testID="featured-logo" className="h-full w-full" resizeMode="contain" />
          </View>
        ) : (
          <Text
            role="heading"
            className={`font-sans font-black text-text-primary uppercase leading-display ${Platform.isTV ? "text-tv-display" : "text-web-display"}`}
          >
            {tile.title}
          </Text>
        )}
        <TitleMeta tile={tile} />
        {tile.plot && (
          <Text
            className={`font-sans text-text-secondary leading-body ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}
            numberOfLines={3}
          >
            {tile.plot}
          </Text>
        )}
        <View className="mt-3 flex-row gap-3">
          <ActionButton label="Play" primary onPress={() => router.push(playHref(featured))} />
          <ActionButton label="More info" onPress={() => router.push(titleHref(tile))} />
        </View>
      </View>
    </View>
  );
}
