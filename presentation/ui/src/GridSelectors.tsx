import { type GenreKind, type SortParameter, useListGenres } from "@selfless/api-client";
import { router, useLocalSearchParams } from "expo-router";
import { useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";

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
  return (
    <View className="relative">
      <Pressable
        className="h-9 flex-row items-center gap-2 rounded-small border border-text-muted bg-scrim px-3 hover:border-focus-ring"
        aria-label={label}
        aria-expanded={open}
        onPress={() => setOpen(!open)}
      >
        <Text className="font-sans font-semibold text-web-meta text-text-primary">{current}</Text>
        <Text className="font-sans text-web-caption text-text-muted">▾</Text>
      </Pressable>
      {open && (
        <ScrollView className="absolute top-10 left-0 z-50 max-h-[360px] min-w-[220px] rounded-medium border border-surface-raised bg-surface p-2">
          {options.map((option) => (
            <Pressable
              key={option.label}
              className="flex-row justify-between gap-4 rounded-small p-2 hover:bg-surface-raised"
              role="menuitem"
              onPress={() => {
                setOpen(false);
                onChange(option.value);
              }}
            >
              <Text className="font-sans text-web-meta text-text-primary">{option.label}</Text>
              {option.count !== undefined && (
                <Text className="font-sans text-web-meta text-text-muted">{option.count}</Text>
              )}
            </Pressable>
          ))}
        </ScrollView>
      )}
    </View>
  );
}

export function GridSelectors({ kind }: { kind: GenreKind }) {
  const { genre, sort } = useGridQuery();
  const { data } = useListGenres({ kind });
  const genres = data?.status === 200 ? data.data.genres : [];
  return (
    <View className="flex-row gap-3">
      <Select
        label="Genre"
        current={genre ?? "All genres"}
        options={[{ label: "All genres" }, ...genres.map((x) => ({ value: x.name, label: x.name, count: x.count }))]}
        onChange={(value) => router.setParams({ genre: value })}
      />
      <Select
        label="Sort"
        current={(sorts.find((x) => x.value === sort) ?? sorts[0]).label}
        options={sorts.map((x) => ({ value: x.value === "title" ? undefined : x.value, label: x.label }))}
        onChange={(value) => router.setParams({ sort: value })}
      />
    </View>
  );
}
