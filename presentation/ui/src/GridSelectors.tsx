import { type GenreKind, type SortParameter, useListGenres } from "@selfless/api-client";
import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { BackHandler, Platform } from "react-native";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";
import { FocusGuide, type FocusGuideInstance } from "./FocusGuide";

const sorts: { value: SortParameter; label: string }[] = [
  { value: "title", label: "Title" },
  { value: "added", label: "Recently added" },
  { value: "released", label: "Release year" },
];

export function useGridQuery(): { genre?: string; sort?: SortParameter } {
  const params = useLocalSearchParams<{ genre?: string; sort?: string }>();
  return {
    genre: params.genre || undefined,
    sort: sorts.find((sort) => sort.value === params.sort)?.value,
  };
}

type Option = { value?: string; label: string; count?: number };

export function Select({
  label,
  current,
  options,
  onChange,
}: {
  label: string;
  current: string;
  options: Option[];
  onChange: (value?: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<FocusGuideInstance>(null);
  const list = useRef<FocusGuideInstance>(null);
  const [currentOption, setCurrentOption] = useState<FocusGuideInstance | null>(null);
  const close = () => {
    setOpen(false);
    if (Platform.isTV) {
      button.current?.requestTVFocus();
    }
  };
  useEffect(() => {
    if (!Platform.isTV || !open) {
      return;
    }
    list.current?.requestTVFocus();
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      setOpen(false);
      button.current?.requestTVFocus();
      return true;
    });
    return () => subscription.remove();
  }, [open, currentOption]);
  const choose = (value?: string) => {
    close();
    onChange(value);
  };
  const options_ = options.map((option) => (
    <Pressable
      key={option.label}
      ref={
        option.label === current ? (node) => setCurrentOption(node as unknown as FocusGuideInstance | null) : undefined
      }
      className={
        Platform.isTV
          ? "group flex-row justify-between gap-6 rounded-small px-5 py-3 focus:bg-button-focus"
          : "flex-row justify-between gap-4 rounded-small p-2 hover:bg-surface-raised"
      }
      role="menuitem"
      onPress={() => choose(option.value)}
    >
      <Text
        className={`font-sans text-text-primary ${Platform.isTV ? "text-tv-body group-focus:text-text-inverse" : "text-web-meta"}`}
      >
        {option.label}
      </Text>
      {option.count !== undefined && (
        <Text
          className={`font-sans text-text-muted ${Platform.isTV ? "text-tv-body group-focus:text-text-inverse" : "text-web-meta"}`}
        >
          {option.count}
        </Text>
      )}
    </Pressable>
  ));
  return (
    <View className="relative">
      <FocusGuide autoFocus ref={button}>
        <Pressable
          className={
            Platform.isTV
              ? "group h-button-tv-height flex-row items-center gap-4 rounded-small bg-button-background px-6 transition-transform focus:scale-105 focus:bg-button-focus"
              : "h-9 flex-row items-center gap-2 rounded-small border border-text-muted bg-scrim px-3 hover:border-focus-ring"
          }
          aria-label={label}
          aria-expanded={open}
          onPress={() => setOpen(!open)}
        >
          <Text
            className={`font-sans font-semibold text-text-primary ${Platform.isTV ? "text-tv-body group-focus:text-text-inverse" : "text-web-meta"}`}
          >
            {current}
          </Text>
          <Text
            className={`font-sans text-text-muted ${Platform.isTV ? "text-tv-body group-focus:text-text-inverse" : "text-web-caption"}`}
          >
            ▾
          </Text>
        </Pressable>
      </FocusGuide>
      {open &&
        (Platform.isTV ? (
          <FocusGuide
            ref={list}
            destinations={currentOption ? [currentOption] : []}
            trapFocusUp
            trapFocusDown
            trapFocusLeft
            trapFocusRight
            className="absolute top-[88px] left-0 z-50 max-h-[640px] min-w-[420px] rounded-medium border border-surface-raised bg-surface p-3"
          >
            <ScrollView>{options_}</ScrollView>
          </FocusGuide>
        ) : (
          <ScrollView className="absolute top-10 left-0 z-50 max-h-[360px] min-w-[220px] rounded-medium border border-surface-raised bg-surface p-2">
            {options_}
          </ScrollView>
        ))}
    </View>
  );
}

export function GridSelectors({ kind }: { kind: GenreKind }) {
  const { genre, sort } = useGridQuery();
  const { data } = useListGenres({ kind });
  const genres = data?.status === 200 ? data.data.genres : [];
  return (
    <View className={`flex-row ${Platform.isTV ? "gap-5" : "gap-3"}`}>
      <Select
        label="Genre"
        current={genre ?? "All genres"}
        options={[
          { label: "All genres" },
          ...genres.map((x) => ({
            value: x.name,
            label: x.name,
            count: x.count,
          })),
        ]}
        onChange={(value) => router.setParams({ genre: value })}
      />
      <Select
        label="Sort"
        current={(sorts.find((x) => x.value === sort) ?? sorts[0]).label}
        options={sorts.map((x) => ({
          value: x.value === "title" ? undefined : x.value,
          label: x.label,
        }))}
        onChange={(value) => router.setParams({ sort: value })}
      />
    </View>
  );
}
