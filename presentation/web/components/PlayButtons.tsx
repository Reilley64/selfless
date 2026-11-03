import { ActionButton } from "@selfless/ui";
import { useRouter } from "expo-router";

export function PlayButtons({ href, resumePosition }: { href: string; resumePosition: number | null }) {
  const router = useRouter();
  if (resumePosition === null) {
    return <ActionButton label="Play" primary onPress={() => router.push(href)} />;
  }
  return (
    <>
      <ActionButton label="Resume" primary onPress={() => router.push(href)} />
      <ActionButton label="Start over" onPress={() => router.push(`${href}?position=0`)} />
    </>
  );
}
