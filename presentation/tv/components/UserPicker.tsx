import type { UserColour } from "@selfless/api-client";
import { FocusGuide } from "@selfless/ui";
import symbols from "@selfless/ui/theme/symbols.json";
import type { ReactNode } from "react";
import { useSyncExternalStore } from "react";
import { SvgXml } from "react-native-svg";
import { Pressable, Text, View } from "react-native-css/components";
import { session } from "../session";

const colours: Record<UserColour, string> = {
  amber: "bg-user-amber",
  blue: "bg-user-blue",
  pink: "bg-user-pink",
  emerald: "bg-user-emerald",
};

function PickerButton({
  label,
  className,
  onPress,
  children,
}: {
  label: string;
  className: string;
  onPress: () => void;
  children: ReactNode;
}) {
  return (
    <Pressable className="items-center gap-6" onPress={onPress}>
      {({ focused }) => (
        <>
          <View
            className={`size-[220px] items-center justify-center rounded-pill border-(length:--spacing-focus-ring-tv-width) ${className} ${focused ? "border-focus-ring" : "border-transparent"}`}
          >
            {children}
          </View>
          <Text className={`font-sans font-bold text-tv-heading ${focused ? "text-text-primary" : "text-text-muted"}`}>
            {label}
          </Text>
        </>
      )}
    </Pressable>
  );
}

export function UserPicker({ onAddUser }: { onAddUser: () => void }) {
  const { users } = useSyncExternalStore(session.subscribe, session.state);

  return (
    <FocusGuide autoFocus className="flex-1 items-center justify-center gap-16 bg-background">
      <Text className="font-sans font-bold text-tv-title text-text-primary">Who&apos;s watching?</Text>
      <View className="flex-row gap-12">
        {users.map((user) => (
          <PickerButton
            key={user.id}
            label={user.name}
            className={colours[user.colour]}
            onPress={() => session.pick(user.id)}
          >
            <SvgXml
              xml={`<svg viewBox="0 0 200 200">${symbols.symbol[user.symbol]}</svg>`}
              width="100%"
              height="100%"
            />
          </PickerButton>
        ))}
        <PickerButton label="Add user" className="bg-surface-raised" onPress={onAddUser}>
          <Text className="font-sans text-tv-display text-text-primary">+</Text>
        </PickerButton>
      </View>
    </FocusGuide>
  );
}
