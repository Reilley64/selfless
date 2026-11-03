import type { Tile as TileData } from "@selfless/api-client";
import { fireEvent, render, screen } from "@testing-library/react-native";
import { PixelRatio, Platform } from "react-native";
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

afterEach(() => {
  jest.restoreAllMocks();
});

const image = (id: string, widths: number[]) => ({
  urls: widths.map((width) => ({ width, url: `/api/images/${id}/${width}/v1?expires=1793836800&signature=s` })),
});
const fanart = image("0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8", [480, 960, 1280, 1920]);
const logo = image("0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9", [300, 600]);
const thumb = image("0190f3c4-9fa0-7b12-c3d4-e5f6a7b8c9d0", [480, 960]);

const severance = { id: "0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6c", title: "Severance", logo };

const woesHollow: TileData = {
  ...arrival,
  id: "0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6b",
  kind: "episode",
  title: "Woe's Hollow",
  year: 2025,
  runtimeMinutes: 57,
  images: { fanart, logo: null, thumb },
  episode: {
    show: severance,
    seasonNumber: 2,
    episodeNumber: 4,
  },
};

async function drawnAt(testId: string, width: number) {
  await fireEvent(screen.getByTestId(testId), "layout", { nativeEvent: { layout: { width, height: width / 2 } } });
  return screen.getByTestId(testId).props.source?.uri as string;
}

test("a tile without fanart shows the dark placeholder with the title", async () => {
  await render(<Tile tile={arrival} />);

  expect(screen.getByText("Arrival")).toBeOnTheScreen();
  expect(screen.queryByTestId("tile-fanart")).toBeNull();
});

test("a tile shows the fanart with the logo laid over it instead of the title", async () => {
  await render(<Tile tile={{ ...arrival, images: { fanart, logo, thumb: null } }} />);

  expect(await drawnAt("tile-fanart", 480)).toContain("/0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8/");
  expect(await drawnAt("tile-logo", 300)).toContain("/0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9/");
  expect(screen.queryByText("Arrival")).toBeNull();
});

test("a tile without a logo shows the title as text over the fanart", async () => {
  await render(<Tile tile={{ ...arrival, images: { fanart, logo: null, thumb: null } }} />);

  expect(screen.getByTestId("tile-fanart")).toBeOnTheScreen();
  expect(screen.queryByTestId("tile-logo")).toBeNull();
  expect(screen.getByText("Arrival")).toBeOnTheScreen();
});

test("on TV, a tile asks for the image width of its layout in design pixels", async () => {
  jest.spyOn(Platform, "isTV", "get").mockReturnValue(true);
  jest.spyOn(PixelRatio, "get").mockReturnValue(2);
  await render(<Tile tile={{ ...arrival, images: { fanart, logo: null, thumb: null } }} />);

  expect(await drawnAt("tile-fanart", 480)).toContain("/480/v1");
});

test("a tile shows the user's progress bar and no watched mark while they are part way through", async () => {
  await render(<Tile tile={{ ...arrival, progress: 0.42 }} />);

  expect(screen.getByRole("progressbar")).toHaveAccessibilityValue({ now: 42 });
  expect(screen.queryByLabelText("Watched")).toBeNull();
});

test("a watched tile shows the watched mark and no progress bar", async () => {
  await render(<Tile tile={{ ...arrival, watched: true }} />);

  expect(screen.getByLabelText("Watched")).toBeOnTheScreen();
  expect(screen.queryByRole("progressbar")).toBeNull();
});

test("an episode tile shows the episode thumb, the show's logo and an S:E badge", async () => {
  await render(<Tile tile={woesHollow} />);

  expect(await drawnAt("tile-thumb", 480)).toContain("/0190f3c4-9fa0-7b12-c3d4-e5f6a7b8c9d0/");
  expect(await drawnAt("tile-logo", 300)).toContain("/0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9/");
  expect(screen.getByText("S2:E4")).toBeOnTheScreen();
});

test("an episode tile without the show's logo shows the show's title", async () => {
  const episode = { show: { ...severance, logo: null }, seasonNumber: 2, episodeNumber: 4 };
  await render(<Tile tile={{ ...woesHollow, episode }} />);

  expect(screen.getByText("Severance")).toBeOnTheScreen();
  expect(screen.queryByTestId("tile-logo")).toBeNull();
});
