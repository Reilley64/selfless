import { Home } from "@selfless/ui";
import { use } from "react";
import { MenuKeepsFocus } from "../../components/Menu";

export default function HomeScreen() {
  return <Home autoFocus={!use(MenuKeepsFocus)} />;
}
