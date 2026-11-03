import { useRouter } from "expo-router";
import { ActionButton } from "./ActionButton";

export function PlayActions({
  href,
  resumePosition,
  onFocus,
}: {
  href: string;
  resumePosition: number | null;
  onFocus?: () => void;
}) {
  const router = useRouter();
  if (resumePosition == null) {
    return <ActionButton label="Play" primary onPress={() => router.push(href)} onFocus={onFocus} />;
  }
  return (
    <>
      <ActionButton label="Resume" primary onPress={() => router.push(href)} onFocus={onFocus} />
      <ActionButton label="Start over" onPress={() => router.push(`${href}?from=start`)} onFocus={onFocus} />
    </>
  );
}
