import type { Home as HomeData, Tile } from "@selfless/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { Platform } from "react-native";
import { Home } from "./Home";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

afterEach(() => {
  jest.restoreAllMocks();
});

const tile = (id: string, kind: Tile["kind"], title: string): Tile => ({
  id,
  kind,
  title,
  year: 2016,
  certification: "PG-13",
  runtimeMinutes: kind === "movie" ? 116 : null,
  seasonCount: kind === "show" ? 2 : null,
  genres: ["Drama"],
  plot: `The plot of ${title}.`,
  images: { fanart: null, logo: null, thumb: null },
  progress: null,
  watched: false,
  episode: null,
});

const arrival = tile("0190f3c4-5b6a-7c8d-9e0f-1a2b3c4d5e6f", "movie", "Arrival");
const severance = tile("0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6b", "show", "Severance");

const home: HomeData = {
  featured: { tile: severance, playEpisodeId: "0190f3c4-9a0b-7c1d-8e2f-3a4b5c6d7e8f" },
  rows: [
    { kind: "recentlyAdded", genre: null, tiles: [arrival] },
    { kind: "genre", genre: "Drama", tiles: [] },
    { kind: "genre", genre: "Science Fiction", tiles: [arrival, severance] },
  ],
};

async function renderHome(response: HomeData = home) {
  jest
    .spyOn(globalThis, "fetch")
    .mockImplementation(async (input) =>
      input === "/api/home/v1" ? Response.json(response) : new Response(null, { status: 404 }),
    );
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });
  await renderRouter(
    {
      index: () => (
        <QueryClientProvider client={client}>
          <Home />
        </QueryClientProvider>
      ),
      "movies/[movieId]": () => null,
      "shows/[showId]": () => null,
    },
    { initialUrl: "/" },
  );
  await screen.findAllByRole("heading");
}

test("Home renders the featured title, then its rows in order, and hides empty rows", async () => {
  await renderHome();

  expect(screen.getAllByRole("heading").map((x) => x.props.children)).toEqual([
    "Severance",
    "Recently added",
    "Science Fiction",
  ]);
});

test("More info on the featured title opens the show page", async () => {
  await renderHome();

  await fireEvent.press(screen.getByText("More info"));

  expect(screen).toHavePathname(`/shows/${severance.id}`);
});

jest.mock("./FocusGuide", () => {
  const { View } = jest.requireActual("react-native");
  return {
    FocusGuide: ({ ref, ...props }: { ref?: { current: unknown } }) => {
      if (ref) {
        ref.current = { requestTVFocus: () => {} };
      }
      return <View {...props} />;
    },
  };
});

describe("on TV", () => {
  beforeEach(() => {
    jest.spyOn(Platform, "isTV", "get").mockReturnValue(true);
  });

  test("focusing a tile shows its details in the top area without another request", async () => {
    await renderHome();

    await fireEvent(screen.getAllByLabelText("Arrival")[0], "focus");

    expect(screen.getByText("Movie")).toBeOnTheScreen();
    expect(screen.getByRole("heading", { name: "Arrival" })).toBeOnTheScreen();
    expect(screen.getByText("1h 56m")).toBeOnTheScreen();
    expect(screen.getByText("The plot of Arrival.")).toBeOnTheScreen();
    expect(globalThis.fetch).toHaveBeenCalledTimes(1);
  });

  test("moving focus back to the featured title hides the focused details", async () => {
    await renderHome();
    await fireEvent(screen.getAllByLabelText("Arrival")[0], "focus");

    await fireEvent(screen.getByText("Play"), "focus");

    expect(screen.queryByText("The plot of Arrival.")).not.toBeOnTheScreen();
    expect(screen.getByText("The plot of Severance.")).toBeOnTheScreen();
  });

  test("an episode tile shows its show, episode number, title and time left", async () => {
    const episode: Tile = {
      ...tile("0190f3c4-7d8e-7f9a-8b0c-1d2e3f4a5b6c", "episode", "Half Loop"),
      runtimeMinutes: 50,
      progress: 0.4,
      episode: { show: { id: severance.id, title: "Severance", logo: null }, seasonNumber: 1, episodeNumber: 2 },
    };
    await renderHome({
      featured: { tile: arrival, playEpisodeId: null },
      rows: [{ kind: "continueWatching", genre: null, tiles: [episode] }],
    });

    await fireEvent(screen.getByLabelText("Half Loop"), "focus");

    expect(screen.getByRole("heading", { name: "Severance" })).toBeOnTheScreen();
    expect(screen.getByText("S1:E2")).toBeOnTheScreen();
    expect(screen.getByText("30 min left")).toBeOnTheScreen();
  });
});

test("a movie tile opens the movie page", async () => {
  await renderHome();

  await fireEvent.press(screen.getAllByLabelText("Arrival")[0]);

  expect(screen).toHavePathname(`/movies/${arrival.id}`);
});
