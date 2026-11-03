import type { Tile as TileData } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { Platform, type ScrollViewInstance } from "react-native";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";
import { ActionButton } from "./ActionButton";
import { ArtworkImage, Tile } from "./Tile";
import { TitleMeta } from "./TitleMeta";

const HOVER_DELAY = 400;
const INSET = 48;
const EXPANDED_WIDTH = 520;

function TileCard({ tile, onPress }: { tile: TileData; onPress?: () => void }) {
  const router = useRouter();
  return (
    <Pressable
      className="aspect-video w-full justify-end overflow-hidden rounded-medium bg-surface px-6 py-5 shadow-2xl ring-3 ring-focus-ring"
      aria-label={tile.title}
      onPress={onPress}
    >
      {tile.images.fanart && (
        <ArtworkImage artwork={tile.images.fanart} testID="card-fanart" className="absolute inset-0" resizeMode="cover" />
      )}
      <View className="absolute inset-0 bg-linear-to-r from-black/90 via-black/65 via-60% to-black/10" />
      <Text className="font-sans font-extrabold text-web-heading text-text-primary leading-title" numberOfLines={1}>
        {tile.title}
      </Text>
      <View className="mt-2">
        <TitleMeta tile={tile} small />
      </View>
      {tile.plot && (
        <Text className="mt-2 font-sans text-web-meta text-text-secondary leading-body" numberOfLines={2}>
          {tile.plot}
        </Text>
      )}
      <View className="mt-3 flex-row gap-2">
        {tile.kind === "movie" && (
          <ActionButton label="Play" primary small onPress={() => router.push(`/play/movies/${tile.id}`)} />
        )}
        {onPress && <ActionButton label="More info" small onPress={onPress} />}
      </View>
    </Pressable>
  );
}

function RowTile({
  tile,
  onPress,
  onOpen,
}: {
  tile: TileData;
  onPress?: () => void;
  onOpen: (left: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const left = useRef(0);
  useEffect(() => () => clearTimeout(timer.current), []);
  const later = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setOpen(true);
      onOpen(left.current);
    }, HOVER_DELAY);
  };
  const close = () => {
    clearTimeout(timer.current);
    setOpen(false);
  };
  const width = Platform.isTV ? "w-tile-tv-width" : open ? "w-tile-web-expanded-width" : "w-tile-web-width";
  return (
    <View
      className={`transition-[width] duration-300 ${width} ${open ? "z-10" : ""}`}
      onLayout={(event) => {
        left.current = event.nativeEvent.layout.x;
      }}
      onPointerEnter={later}
      onPointerLeave={close}
    >
      {open ? <TileCard tile={tile} onPress={onPress} /> : <Tile tile={tile} onPress={onPress} />}
    </View>
  );
}

function Arrow({ side, onPress }: { side: "left" | "right"; onPress: () => void }) {
  return (
    <Pressable
      className={`absolute top-2 h-tile-web-height w-web-inset items-center justify-center bg-background/70 opacity-0 transition-opacity group-hover:opacity-100 hover:bg-background/90 ${side === "left" ? "left-0" : "right-0"}`}
      aria-label={side === "left" ? "Scroll left" : "Scroll right"}
      onPress={onPress}
    >
      <Text className="font-sans font-bold text-web-heading text-text-primary">{side === "left" ? "‹" : "›"}</Text>
    </Pressable>
  );
}

export function TileRow({
  title,
  tiles,
  onPressTile,
}: {
  title: string;
  tiles: TileData[];
  onPressTile?: (tile: TileData) => void;
}) {
  const scroll = useRef<ScrollViewInstance>(null);
  const [offset, setOffset] = useState(0);
  const [viewport, setViewport] = useState(0);
  const [content, setContent] = useState(0);
  const scrollTo = (x: number) => scroll.current?.scrollTo({ x: Math.max(0, x), animated: true });
  const page = viewport - 2 * INSET;
  return (
    <View className="group mb-6">
      <Text
        role="heading"
        className={`mb-2 font-sans font-bold text-text-secondary ${Platform.isTV ? "px-tv-inset text-tv-row" : "px-web-inset text-web-row"}`}
      >
        {title}
      </Text>
      <View className="relative">
        <ScrollView
          ref={scroll}
          horizontal
          showsHorizontalScrollIndicator={false}
          scrollEventThrottle={16}
          onScroll={(event) => setOffset(event.nativeEvent.contentOffset.x)}
          onLayout={(event) => setViewport(event.nativeEvent.layout.width)}
          onContentSizeChange={(width) => setContent(width)}
          contentContainerClassName={`items-start py-2 ${Platform.isTV ? "gap-tile-tv-gap px-tv-inset" : "gap-tile-web-gap px-web-inset"}`}
        >
          {tiles.map((tile) => (
            <RowTile
              key={tile.id}
              tile={tile}
              onPress={onPressTile && (() => onPressTile(tile))}
              onOpen={(left) => {
                const overflow = left - offset + EXPANDED_WIDTH - (viewport - INSET);
                if (overflow > 0) {
                  scrollTo(offset + overflow);
                }
              }}
            />
          ))}
        </ScrollView>
        {!Platform.isTV && offset > 2 && <Arrow side="left" onPress={() => scrollTo(offset - page)} />}
        {!Platform.isTV && offset + viewport < content - 2 && (
          <Arrow side="right" onPress={() => scrollTo(offset + page)} />
        )}
      </View>
    </View>
  );
}
