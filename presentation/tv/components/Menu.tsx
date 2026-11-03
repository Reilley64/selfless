import Feather from "@expo/vector-icons/Feather";
import { FocusGuide, type FocusGuideInstance, initials } from "@selfless/ui";
import type { Href } from "expo-router";
import { createContext, type Ref, useImperativeHandle, useRef, useState, useSyncExternalStore } from "react";
import { styled } from "react-native-css";
import { Pressable, Text, View } from "react-native-css/components";
import { session } from "../session";
import { userColours } from "./UserPicker";

const Icon = styled(Feather);

const pages = [
  { path: "/search", label: "Search", icon: "search" },
  { path: "/", label: "Home", icon: "home" },
  { path: "/shows", label: "Shows", icon: "tv" },
  { path: "/movies", label: "Movies", icon: "film" },
] as const;

const settings = { path: "/settings", label: "Settings", icon: "settings" } as const;

export type MenuInstance = { focus(path: string): void };

export const MenuKeepsFocus = createContext(false);

export function Menu({
  ref,
  current,
  onFocusChange,
  onSelect,
}: {
  ref: Ref<MenuInstance>;
  current: string;
  onFocusChange: (focused: boolean) => void;
  onSelect: (path: Href) => void;
}) {
  const { users, currentUserId } = useSyncExternalStore(session.subscribe, session.state);
  const user = users.find(({ id }) => id === currentUserId);
  const items = useRef<Record<string, FocusGuideInstance | null>>({});
  const [currentItem, setCurrentItem] = useState<FocusGuideInstance | null>(null);
  const [open, setOpen] = useState(false);
  useImperativeHandle(ref, () => ({ focus: (path) => items.current[path]?.requestTVFocus() }), []);
  const focusChanged = (focused: boolean) => {
    setOpen(focused);
    onFocusChange(focused);
  };
  const item = (page: (typeof pages)[number] | typeof settings) => (
    <Pressable
      key={page.path}
      ref={(node) => {
        items.current[page.path] = node as unknown as FocusGuideInstance | null;
        if (node && page.path === current) {
          setCurrentItem(items.current[page.path]);
        }
      }}
      className="h-[72px] flex-row items-center gap-[26px] self-start rounded-pill px-5 focus:scale-106 focus:bg-button-focus"
      aria-label={page.label}
      onPress={() => onSelect(page.path)}
      onFocus={() => focusChanged(true)}
      onBlur={() => focusChanged(false)}
    >
      {({ focused }) => (
        <>
          <Icon
            name={page.icon}
            size={38}
            className={focused ? "text-text-inverse" : page.path === current ? "text-accent" : "text-text-muted"}
          />
          {open && (
            <Text
              className={`font-sans font-bold text-[34px] ${focused ? "text-text-inverse" : page.path === current ? "text-text-primary" : "text-text-muted"}`}
            >
              {page.label}
            </Text>
          )}
          {!open && page.path === current && (
            <View className="absolute bottom-1.5 left-[34px] h-1 w-2.5 rounded-small bg-accent" />
          )}
        </>
      )}
    </Pressable>
  );

  return (
    <FocusGuide
      destinations={currentItem ? [currentItem] : []}
      trapFocusUp
      trapFocusDown
      className={`absolute inset-y-0 left-0 z-20 ${open ? "right-0" : "w-nav-tv-width"}`}
    >
      <View
        className={`absolute inset-y-0 left-0 ${open ? "right-0 bg-linear-to-r from-black/95 via-black/85 via-25% to-black/20" : "w-[220px] bg-linear-to-r from-black/85 to-transparent"}`}
      />
      <View className="absolute top-14 left-6">{item(settings)}</View>
      {user && (
        <Pressable
          className="absolute top-[142px] left-6 h-[72px] flex-row items-center gap-5 self-start rounded-pill pr-5 pl-1.5 focus:scale-106 focus:bg-button-focus"
          onPress={() => session.switchUser()}
          onFocus={() => focusChanged(true)}
          onBlur={() => focusChanged(false)}
        >
          {({ focused }) => (
            <>
              <View className={`size-14 items-center justify-center rounded-pill ${userColours[user.colour]}`}>
                <Text className="font-sans font-bold text-tv-label text-text-primary">{initials(user.name)}</Text>
              </View>
              {open && (
                <View>
                  <Text
                    className={`font-sans font-bold text-tv-row ${focused ? "text-text-inverse" : "text-text-primary"}`}
                  >
                    {user.name}
                  </Text>
                  <Text
                    className={`font-sans font-semibold text-tv-caption ${focused ? "text-text-inverse" : "text-text-secondary"}`}
                  >
                    Switch user
                  </Text>
                </View>
              )}
            </>
          )}
        </Pressable>
      )}
      <View className="absolute inset-y-0 left-6 justify-center gap-3.5">{pages.map(item)}</View>
    </FocusGuide>
  );
}
