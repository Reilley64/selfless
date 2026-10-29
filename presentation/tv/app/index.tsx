import { FocusGuide, ServerInfo } from "@selfless/ui";
import { Pressable, Text } from "react-native-css/components";
import { session } from "../session";

export default function Home() {
  return (
    <FocusGuide autoFocus className="flex-1 items-start gap-8 bg-background p-tv-inset">
      <ServerInfo />
      <Pressable
        className="h-button-tv-height justify-center rounded-small bg-button-background px-8 focus:bg-button-focus"
        onPress={() => session.switchUser()}
      >
        {({ focused }) => (
          <Text
            className={`font-sans font-semibold text-tv-body ${focused ? "text-text-inverse" : "text-text-primary"}`}
          >
            Switch user
          </Text>
        )}
      </Pressable>
    </FocusGuide>
  );
}
