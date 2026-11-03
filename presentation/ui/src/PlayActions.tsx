import { useRouter } from "expo-router";
import { ActionButton } from "./ActionButton";

export function PlayActions({ href, resumePosition }: { href: string; resumePosition: number | null }) {
  const router = useRouter();
  if (resumePosition == null) {
    return <ActionButton label="Play" primary onPress={() => router.push(href)} />;
  }
  return (
    <>
      <ActionButton label="Resume" primary onPress={() => router.push(href)} />
      <ActionButton label="Start over" onPress={() => router.push(`${href}?from=start`)} />
    </>
  );
}
