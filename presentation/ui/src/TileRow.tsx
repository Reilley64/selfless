import type { Tile as TileData } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { type Ref, useEffect, useRef, useState } from "react";
import { type View as HostView, Platform, type ScrollViewInstance } from "react-native";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";
import { ActionButton } from "./ActionButton";
import { episodeLabel } from "./episodeLabel";
import type { FocusGuideInstance } from "./FocusGuide";
import { playHref } from "./playHref";
import { ArtworkImage, Tile } from "./Tile";
import { TileMenu, type TileMenuItem } from "./TileMenu";
import { tilePath } from "./tilePath";
import { TitleMeta } from "./TitleMeta";

const HOVER_DELAY = 400;
const INSET = 48;
const EXPANDED_WIDTH = 520;

function TileCard({ tile, onPress, onMenu }: { tile: TileData; onPress?: () => void; onMenu?: () => void }) {
  const router = useRouter();
  const { episode } = tile;
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
        {episode ? episode.show.title : tile.title}
      </Text>
      <View className="mt-2">
        <TitleMeta tile={tile} small />
      </View>
      {episode && (
        <Text className="mt-2 font-sans text-web-meta text-text-primary" numberOfLines={1}>
          <Text className="font-bold">{episodeLabel(episode.seasonNumber, episode.episodeNumber)}</Text> {tile.title}
        </Text>
      )}
      {tile.plot && (
        <Text className="mt-2 font-sans text-web-meta text-text-secondary leading-body" numberOfLines={2}>
          {tile.plot}
        </Text>
      )}
      <View className="mt-3 flex-row gap-2">
        {tile.kind !== "show" && (
          <ActionButton
            label="Play"
            primary
            small
            onPress={() => router.push(playHref(tile))}
          />
        )}
        {onPress && <ActionButton label="More info" small onPress={() => router.push(tilePath(tile))} />}
        {onMenu && (
          <Pressable
            className="ml-auto h-[34px] w-[34px] items-center justify-center rounded-full bg-button-background hover:bg-button-focus"
            aria-label="More actions"
            onPress={onMenu}
          >
            <Text className="font-bold font-sans text-text-primary text-web-meta">⋯</Text>
          </Pressable>
        )}
      </View>
    </Pressable>
  );
}

function RowTile({
  tileRef,
  tile,
  onPress,
  onMenu,
  onOpen,
  onFocus,
  onPlaced,
}: {
  tileRef: Ref<HostView>;
  tile: TileData;
  onPress?: () => void;
  onMenu?: () => void;
  onOpen: (left: number) => void;
  onFocus: (left: number) => void;
  onPlaced?: (left: number) => void;
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
  const contextMenu = onMenu && {
    onContextMenu: (event: { preventDefault: () => void }) => {
      event.preventDefault();
      onMenu();
    },
  };
  return (
    <View
      {...contextMenu}
      className={`transition-[width] duration-300 ${width} ${open ? "z-10" : ""}`}
      onLayout={(event) => {
        left.current = event.nativeEvent.layout.x;
        onPlaced?.(left.current);
      }}
      onPointerEnter={later}
      onPointerLeave={close}
    >
      {open ? (
        <TileCard tile={tile} onPress={onPress} onMenu={onMenu} />
      ) : (
        <Tile
          ref={tileRef}
          tile={tile}
          onPress={onPress}
          onLongPress={onMenu}
          onFocus={() => onFocus(left.current)}
        />
      )}
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
  onFocusTile,
  menu,
}: {
  title: string;
  tiles: TileData[];
  onPressTile?: (tile: TileData) => void;
  onFocusTile?: (tile: TileData) => void;
  menu?: (tile: TileData) => TileMenuItem[];
}) {
  const scroll = useRef<ScrollViewInstance>(null);
  const [menuTile, setMenuTile] = useState<TileData | null>(null);
  const tileViews = useRef(new Map<string, HostView>());
  const chosen = useRef<{ id: string; index: number } | null>(null);
  const firstLeft = useRef(0);
  const [offset, setOffset] = useState(0);
  const [viewport, setViewport] = useState(0);
  const [content, setContent] = useState(0);
  useEffect(() => {
    const from = chosen.current;
    chosen.current = null;
    if (Platform.isTV && from && tiles.length > 0 && !tiles.some((tile) => tile.id === from.id)) {
      const next = tiles[Math.min(from.index, tiles.length - 1)];
      (tileViews.current.get(next.id) as unknown as FocusGuideInstance | undefined)?.requestTVFocus();
    }
  }, [tiles]);
  const scrollTo = (x: number) => scroll.current?.scrollTo({ x: Math.max(0, x), animated: true });
  const page = viewport - 2 * INSET;
  return (
    <View className={Platform.isTV ? "mb-6" : "group mb-6"}>
      <Text
        role="heading"
        className={`mb-2 font-sans font-bold text-text-secondary ${Platform.isTV ? "px-tv-inset text-tv-row" : "px-web-inset text-web-row"}`}
      >
        {title}
      </Text>
      <View className={Platform.isTV ? "pl-tv-inset" : "relative"}>
        <ScrollView
          ref={scroll}
          horizontal
          showsHorizontalScrollIndicator={false}
          scrollEventThrottle={16}
          onScroll={(event) => setOffset(event.nativeEvent.contentOffset.x)}
          onLayout={(event) => setViewport(event.nativeEvent.layout.width)}
          onContentSizeChange={(width) => setContent(width)}
          className={Platform.isTV ? "-ml-4" : ""}
          contentContainerClassName={`items-start py-2 ${Platform.isTV ? "gap-tile-tv-gap pr-tv-inset pl-4" : "gap-tile-web-gap px-web-inset"}`}
        >
          {tiles.map((tile, index) => (
            <RowTile
              key={tile.id}
              tileRef={(view) => {
                if (view) {
                  tileViews.current.set(tile.id, view);
                }
                return () => {
                  tileViews.current.delete(tile.id);
                };
              }}
              tile={tile}
              onPress={onPressTile && (() => onPressTile(tile))}
              onMenu={menu && (() => setMenuTile(tile))}
              onPlaced={
                index === 0
                  ? (left) => {
                      firstLeft.current = left;
                    }
                  : undefined
              }
              onFocus={(left) => {
                if (Platform.isTV) {
                  scrollTo(left - firstLeft.current);
                }
                onFocusTile?.(tile);
              }}
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
      {menu && menuTile && (
        <TileMenu
          title={menuTile.episode ? menuTile.episode.show.title : menuTile.title}
          items={menu(menuTile).map((item) => ({
            ...item,
            onPress: () => {
              chosen.current = { id: menuTile.id, index: tiles.indexOf(menuTile) };
              item.onPress();
            },
          }))}
          onClose={() => setMenuTile(null)}
        />
      )}
    </View>
  );
}
