import { View } from "react-native-css/components";

const fills = [
  "w-0",
  "w-[5%]",
  "w-[10%]",
  "w-[15%]",
  "w-[20%]",
  "w-[25%]",
  "w-[30%]",
  "w-[35%]",
  "w-[40%]",
  "w-[45%]",
  "w-[50%]",
  "w-[55%]",
  "w-[60%]",
  "w-[65%]",
  "w-[70%]",
  "w-[75%]",
  "w-[80%]",
  "w-[85%]",
  "w-[90%]",
  "w-[95%]",
  "w-full",
] as const;

export function ProgressBar({ progress, className = "" }: { progress: number; className?: string }) {
  return (
    <View
      accessible
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
      className={`h-1 overflow-hidden rounded-full bg-progress-track ${className}`}
    >
      <View className={`h-full bg-progress-fill ${fills[Math.round(progress * (fills.length - 1))]}`} />
    </View>
  );
}
