import { FocusGuide, ServerInfo } from "@selfless/ui";

export default function Home() {
  return (
    <FocusGuide autoFocus className="flex-1 bg-background p-tv-inset">
      <ServerInfo />
    </FocusGuide>
  );
}
