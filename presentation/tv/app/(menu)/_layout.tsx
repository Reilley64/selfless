import { type Href, Slot, useFocusEffect, usePathname, useRouter } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import { BackHandler, TVEventControl } from "react-native";
import { View } from "react-native-css/components";
import { Menu, type MenuInstance, MenuKeepsFocus } from "../../components/Menu";

export default function MenuLayout() {
  const router = useRouter();
  const pathname = usePathname();
  const menu = useRef<MenuInstance>(null);
  const [menuFocused, setMenuFocused] = useState(false);
  const [menuKeepsFocus, setMenuKeepsFocus] = useState(false);

  useEffect(() => {
    if (menuKeepsFocus) {
      menu.current?.focus(pathname);
    }
  }, [menuKeepsFocus, pathname]);

  useFocusEffect(
    useCallback(() => {
      if (menuFocused && pathname === "/") {
        TVEventControl.disableTVMenuKey();
      } else {
        TVEventControl.enableTVMenuKey();
      }
      const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
        if (!menuFocused) {
          menu.current?.focus(pathname);
          return true;
        }
        if (pathname !== "/") {
          setMenuKeepsFocus(true);
          router.replace("/");
          return true;
        }
        return false;
      });
      return () => {
        subscription.remove();
        TVEventControl.enableTVMenuKey();
      };
    }, [menuFocused, pathname, router]),
  );

  return (
    <View className="flex-1 bg-background">
      <MenuKeepsFocus value={menuKeepsFocus}>
        <Slot />
      </MenuKeepsFocus>
      <Menu
        ref={menu}
        current={pathname}
        onFocusChange={setMenuFocused}
        onSelect={(path: Href) => {
          setMenuKeepsFocus(false);
          router.replace(path);
        }}
      />
    </View>
  );
}
