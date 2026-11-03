import type { Artwork } from "@selfless/api-client";
import type { ReactNode } from "react";
import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";
import { ProgressBar } from "./ProgressBar";
import { ArtworkImage } from "./Tile";

export type DetailItem = { text: string; outlined?: boolean };
export type DetailFact = { label: string; value: string };
export type DetailResume = { progress: number | null; minutesLeft: number | null };

export function DetailHeader({
  kicker,
  title,
  backdrop,
  logo,
  details,
  subtitle,
  plot,
  facts,
  resume,
  children,
}: {
  kicker: string;
  title: string;
  subtitle?: string;
  backdrop: Artwork | null;
  logo: Artwork | null;
  details: DetailItem[];
  plot: string | null;
  facts: DetailFact[];
  resume?: DetailResume;
  children?: ReactNode;
}) {
  const tv = Platform.isTV;
  return (
    <View
      className={`relative w-full justify-end overflow-hidden bg-background ${tv ? "min-h-billboard-tv-height px-tv-inset pb-12" : "min-h-[560px] px-web-inset pb-10"}`}
    >
      {backdrop && (
        <ArtworkImage artwork={backdrop} testID="detail-backdrop" className="absolute inset-0" resizeMode="cover" />
      )}
      <View className="absolute inset-0 bg-linear-to-r from-background via-background/60 to-transparent" />
      <View className="absolute inset-0 bg-linear-to-t from-background via-transparent to-transparent" />
      <View className={`gap-3 ${tv ? "w-[900px]" : "w-full max-w-[640px] pt-24"}`}>
        <Text
          className={`font-sans font-extrabold text-accent uppercase tracking-[0.22em] ${tv ? "text-tv-label" : "text-web-label"}`}
        >
          {kicker}
        </Text>
        {logo ? (
          <View role="img" aria-label={title} className={tv ? "h-[160px] w-[480px]" : "h-[120px] w-[360px]"}>
            <ArtworkImage
              artwork={logo}
              testID="detail-logo"
              className="h-full w-full"
              resizeMode="contain"
            />
          </View>
        ) : (
          <Text
            className={`font-sans font-extrabold text-text-primary leading-display ${tv ? "text-tv-display" : "text-web-display"}`}
          >
            {title}
          </Text>
        )}
        {subtitle && (
          <Text className={`font-sans font-bold text-text-primary ${tv ? "text-tv-row" : "text-web-heading"}`}>
            {subtitle}
          </Text>
        )}
        <View className="flex-row flex-wrap items-center gap-3">
          {details.map(({ text, outlined }) => (
            <Text
              key={text}
              className={`font-sans text-text-secondary ${tv ? "text-tv-meta" : "text-web-meta"} ${outlined ? "rounded-small border border-text-muted px-1" : ""}`}
            >
              {text}
            </Text>
          ))}
        </View>
        {resume?.progress != null && resume.minutesLeft != null && (
          <View className={`flex-row items-center ${tv ? "gap-4" : "gap-3"}`}>
            <ProgressBar progress={resume.progress} className={tv ? "h-1.5 w-[260px]" : "h-1 w-[180px]"} />
            <Text className={`font-sans text-text-secondary ${tv ? "text-tv-meta" : "text-web-meta"}`}>
              {`${resume.minutesLeft} min left`}
            </Text>
          </View>
        )}
        {plot && (
          <Text className={`font-sans text-text-secondary leading-body ${tv ? "text-tv-body" : "text-web-body"}`}>
            {plot}
          </Text>
        )}
        {facts.map(({ label, value }) => (
          <View key={label} className="flex-row gap-2">
            <Text className={`font-sans text-text-muted ${tv ? "text-tv-meta" : "text-web-meta"}`}>{label}</Text>
            <Text className={`shrink font-sans text-text-secondary ${tv ? "text-tv-meta" : "text-web-meta"}`}>
              {value}
            </Text>
          </View>
        ))}
        {children && <View className="mt-3 flex-row gap-3">{children}</View>}
      </View>
    </View>
  );
}
