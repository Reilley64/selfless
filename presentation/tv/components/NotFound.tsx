import { ActionButton, FocusGuide } from "@selfless/ui";
import { useRouter } from "expo-router";
import { Text } from "react-native-css/components";
import { useFirstFieldFocus } from "./form";

export function NotFound({ message = "Nothing here" }: { message?: string }) {
  const router = useRouter();
  const guide = useFirstFieldFocus();
  return (
    <FocusGuide autoFocus ref={guide} className="flex-1 items-start justify-center gap-8 bg-background px-tv-inset">
      <Text role="heading" className="font-sans font-extrabold text-tv-title text-text-primary">
        {message}
      </Text>
      <ActionButton label="Back" onPress={() => router.back()} />
    </FocusGuide>
  );
}
