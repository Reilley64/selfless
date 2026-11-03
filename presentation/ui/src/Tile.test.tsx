import type { Tile as TileData } from "@selfless/api-client";
import { render, screen } from "@testing-library/react-native";
import { Tile } from "./Tile";

const arrival: TileData = {
  id: "0190f3c4-5b6a-7c8d-9e0f-1a2b3c4d5e6f",
  kind: "movie",
  title: "Arrival",
  year: 2016,
  certification: "PG-13",
  runtimeMinutes: 116,
  seasonCount: null,
  genres: ["Drama", "Science Fiction"],
  plot: "A linguist works with the military to talk to aliens.",
  images: { fanart: null, logo: null, thumb: null },
  progress: null,
  watched: false,
  episode: null,
};

test("a tile without fanart shows the dark placeholder with the title", async () => {
  await render(<Tile tile={arrival} />);

  expect(screen.getByText("Arrival")).toBeOnTheScreen();
});
