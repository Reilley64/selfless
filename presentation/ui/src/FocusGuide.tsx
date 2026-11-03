import type { ComponentProps, ReactNode, Ref } from "react";
import { TVFocusGuideView, View } from "react-native";
import { styled } from "react-native-css";

export type FocusGuideInstance = { requestTVFocus(): void };

const StyledFocusGuide = styled(TVFocusGuideView ?? View);

export const FocusGuide = StyledFocusGuide as (
  props: ComponentProps<typeof StyledFocusGuide> & { ref?: Ref<FocusGuideInstance> },
) => ReactNode;
