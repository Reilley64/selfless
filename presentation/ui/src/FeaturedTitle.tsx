import type { FeaturedTitle as FeaturedTitleData } from "@selfless/api-client";
import { useRouter } from "expo-router";
import type { Ref } from "react";
import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";
import { ActionButton } from "./ActionButton";
import { FocusGuide, type FocusGuideInstance } from "./FocusGuide";
import { ArtworkImage } from "./Tile";
import { TitleMeta } from "./TitleMeta";
import { playHref } from "./playHref";
import { tilePath } from "./tilePath";

export function FeaturedTitle({
  featured,
  onFocus,
  ref,
}: {
  featured: FeaturedTitleData;
  onFocus?: () => void;
  ref?: Ref<FocusGuideInstance>;
}) {
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
      <View className={`gap-3 ${Platform.isTV ? "w-[1050px] px-tv-inset pb-[280px]" : "w-[620px] max-w-[50%] px-web-inset pb-[200px]"}`}>
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
        <FocusGuide autoFocus ref={ref} className="mt-3 flex-row gap-3">
          <ActionButton label="Play" primary onPress={() => router.push(playHref(featured))} onFocus={onFocus} />
          <ActionButton label="More info" onPress={() => router.push(tilePath(tile))} onFocus={onFocus} />
        </FocusGuide>
      </View>
    </View>
  );
}
