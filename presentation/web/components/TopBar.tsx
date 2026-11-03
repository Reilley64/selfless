import { useGetMe, type UserColour } from "@selfless/api-client";
import { initials } from "@selfless/ui";
import { Link, usePathname, useRouter } from "expo-router";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { session } from "../session";

const colours: Record<UserColour, string> = {
  amber: "bg-user-amber",
  blue: "bg-user-blue",
  pink: "bg-user-pink",
  emerald: "bg-user-emerald",
};

const pages = [
  { label: "Home", href: "/" },
  { label: "Shows", href: "/shows" },
  { label: "Movies", href: "/movies" },
] as const;

function NavLink({ label, href, current }: { label: string; href: string; current: boolean }) {
  return (
    <Link href={href} asChild>
      <Pressable className="gap-1">
        <Text
          className={`font-sans text-web-body ${current ? "font-bold text-text-primary" : "font-semibold text-text-muted"}`}
        >
          {label}
        </Text>
        <View className={`h-[3px] w-3 rounded-small ${current ? "bg-accent" : "bg-transparent"}`} />
      </Pressable>
    </Link>
  );
}

function UserMenu() {
  const router = useRouter();
  const { data: me } = useGetMe();
  const [open, setOpen] = useState(false);
  if (me?.status !== 200) {
    return null;
  }
  const avatar = `size-8 items-center justify-center rounded-pill ${colours[me.data.colour]}`;
  const item = "rounded-small p-2";
  const signOut = () => {
    setOpen(false);
    router.replace("/");
    void session.signOut();
  };
  return (
    <View className="relative z-50">
      <Pressable className={avatar} aria-label="User menu" onPress={() => setOpen(!open)}>
        <Text className="font-sans font-bold text-web-caption text-text-primary">{initials(me.data.name)}</Text>
      </Pressable>
      {open && (
        <View className="absolute top-10 right-0 w-[220px] rounded-medium border border-surface-raised bg-surface p-2">
          <View className="mb-2 flex-row items-center gap-3 border-b border-surface-raised p-2">
            <View className={avatar}>
              <Text className="font-sans font-bold text-web-caption text-text-primary">{initials(me.data.name)}</Text>
            </View>
            <View>
              <Text className="font-sans font-bold text-web-meta text-text-primary">{me.data.name}</Text>
              <Text className="font-sans text-web-caption text-text-muted">Signed in</Text>
            </View>
          </View>
          <Link href="/settings" asChild>
            <Pressable className={item} onPress={() => setOpen(false)}>
              <Text className="font-sans text-web-meta text-text-primary">Settings</Text>
            </Pressable>
          </Link>
          <Pressable className={item} onPress={signOut}>
            <Text className="font-sans text-web-meta text-text-primary">Sign out</Text>
          </Pressable>
        </View>
      )}
    </View>
  );
}

export function TopBar() {
  const router = useRouter();
  const pathname = usePathname();
  const [query, setQuery] = useState("");
  return (
    <View className="z-50 h-nav-web-height flex-row items-center gap-6 bg-background px-web-inset">
      <Link href="/" asChild>
        <Pressable className="flex-row items-center gap-2">
          <View className="size-[30px] items-center justify-center rounded-small bg-accent">
            <Text className="font-sans font-bold text-web-body text-text-primary">S</Text>
          </View>
          <Text className="font-sans font-bold text-web-row text-text-primary">selfless</Text>
        </Pressable>
      </Link>
      <View className="flex-row gap-6">
        {pages.map((page) => (
          <NavLink
            key={page.href}
            {...page}
            current={page.href === "/" ? pathname === "/" : pathname.startsWith(page.href)}
          />
        ))}
      </View>
      <TextInput
        className="ml-auto h-9 w-[180px] rounded-pill bg-surface-raised px-3 font-sans text-web-meta text-text-primary placeholder:text-text-muted"
        placeholder="Search"
        aria-label="Search"
        value={query}
        onChangeText={setQuery}
        onSubmitEditing={() => router.push({ pathname: "/search", params: { q: query } })}
      />
      <UserMenu />
    </View>
  );
}
