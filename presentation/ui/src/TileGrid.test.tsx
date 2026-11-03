import type { Tile as TileData } from "@selfless/api-client";
import { fireEvent, render, screen } from "@testing-library/react-native";
import { TileGrid } from "./TileGrid";

const tiles: TileData[] = Array.from({ length: 15 }, (_, index) => ({
  id: `0190f3c4-5b6a-7c8d-9e0f-${String(index).padStart(12, "0")}`,
  kind: "movie",
  title: `Movie ${index}`,
  year: 2016,
  certification: null,
  runtimeMinutes: 116,
  seasonCount: null,
  genres: [],
  plot: null,
  images: { fanart: null, logo: null, thumb: null },
  progress: null,
  watched: false,
  episode: null,
}));

test("focusing a tile in the last two rows loads the next page", async () => {
  const onEndReached = jest.fn();
  await render(<TileGrid tiles={tiles} numColumns={5} onEndReached={onEndReached} />);
  onEndReached.mockClear();

  await fireEvent(screen.getByLabelText("Movie 4"), "focus");
  expect(onEndReached).not.toHaveBeenCalled();

  await fireEvent(screen.getByLabelText("Movie 5"), "focus");
  expect(onEndReached).toHaveBeenCalledTimes(1);
});
