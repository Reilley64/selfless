import { Modal, Platform } from "react-native";
import { Pressable, Text, View } from "react-native-css/components";

export type TileMenuItem = { label: string; onPress: () => void };

export function TileMenu({ title, items, onClose }: { title: string; items: TileMenuItem[]; onClose: () => void }) {
  return (
    <Modal transparent visible onRequestClose={onClose}>
      <View className="flex-1 items-center justify-center bg-scrim">
        {!Platform.isTV && <Pressable className="absolute inset-0" aria-label="Close menu" onPress={onClose} />}
        <View
          role="menu"
          aria-label={title}
          className={`rounded-medium bg-surface p-2 ${Platform.isTV ? "min-w-[560px]" : "min-w-[320px]"}`}
        >
          <Text
            className={`px-4 py-2 font-bold font-sans text-text-muted ${Platform.isTV ? "text-tv-caption" : "text-web-meta"}`}
            numberOfLines={1}
          >
            {title}
          </Text>
          {items.map((item, index) => (
            <Pressable
              key={item.label}
              role="menuitem"
              hasTVPreferredFocus={index === 0}
              className="group rounded-small px-4 py-3 hover:bg-button-focus focus:bg-button-focus"
              onPress={() => {
                onClose();
                item.onPress();
              }}
            >
              <Text
                className={`font-sans font-bold text-text-primary group-hover:text-text-inverse group-focus:text-text-inverse ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}
              >
                {item.label}
              </Text>
            </Pressable>
          ))}
        </View>
      </View>
    </Modal>
  );
}
