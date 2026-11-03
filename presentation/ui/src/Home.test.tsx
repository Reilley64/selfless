import type { Home as HomeData, Tile } from "@selfless/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, renderRouter, screen, waitFor } from "expo-router/testing-library";
import { Platform, View } from "react-native";
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
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity }, mutations: { gcTime: Infinity } } });
  await renderRouter(
    {
      index: () => (
        <QueryClientProvider client={client}>
          <Home />
        </QueryClientProvider>
      ),
      "movies/[movieId]": () => null,
      "shows/[showId]": () => null,
      "play/episodes/[episodeId]": () => null,
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
    expect(screen.getByText("S1:E2 Half Loop")).toBeOnTheScreen();
    expect(screen.getByText("30 min left")).toBeOnTheScreen();
  });
});

test("hovering an episode tile opens a card with its show, episode number and title, and Play plays the episode", async () => {
  const episode: Tile = {
    ...tile("0190f3c4-7d8e-7f9a-8b0c-1d2e3f4a5b6c", "episode", "Half Loop"),
    episode: { show: { id: severance.id, title: "Severance", logo: null }, seasonNumber: 1, episodeNumber: 2 },
  };
  await renderHome({
    featured: null,
    rows: [{ kind: "continueWatching", genre: null, tiles: [episode] }],
  });
  jest.useFakeTimers();

  await fireEvent(screen.getByLabelText("Half Loop"), "pointerEnter");
  await act(() => jest.advanceTimersByTime(400));
  jest.useRealTimers();

  expect(screen.getByText("S1:E2 Half Loop")).toBeOnTheScreen();
  await fireEvent.press(screen.getByText("Play"));
  expect(screen).toHavePathname(`/play/episodes/${episode.id}`);
});

test("a movie tile opens the movie page", async () => {
  await renderHome();

  await fireEvent.press(screen.getAllByLabelText("Arrival")[0]);

  expect(screen).toHavePathname(`/movies/${arrival.id}`);
});

describe("the Continue watching tile menu", () => {
  const halfLoop: Tile = {
    ...tile("0190f3c4-7d8e-7f9a-8b0c-1d2e3f4a5b6c", "episode", "Half Loop"),
    episode: { show: { id: severance.id, title: "Severance", logo: null }, seasonNumber: 1, episodeNumber: 2 },
  };
  const dune = tile("0190f3c4-8e9f-7a0b-9c1d-2e3f4a5b6c7d", "movie", "Dune");
  const withContinueWatching: HomeData = {
    featured: null,
    rows: [
      { kind: "continueWatching", genre: null, tiles: [arrival, halfLoop] },
      { kind: "recentlyAdded", genre: null, tiles: [dune] },
    ],
  };

  async function renderWithPendingRemovals() {
    await renderHome(withContinueWatching);
    jest
      .mocked(globalThis.fetch)
      .mockImplementation(async (_, init) =>
        init?.method === "DELETE" ? new Promise<Response>(() => {}) : Response.json(withContinueWatching),
      );
  }

  const deletes = () =>
    jest
      .mocked(globalThis.fetch)
      .mock.calls.filter(([, init]) => init?.method === "DELETE")
      .map(([input]) => input);

  test("a right click opens the menu, and Remove from Continue watching drops the tile at once", async () => {
    await renderWithPendingRemovals();
    const preventDefault = jest.fn();

    await fireEvent(screen.getByLabelText("Arrival"), "contextMenu", { preventDefault });
    await fireEvent.press(screen.getByText("Remove from Continue watching"));

    expect(preventDefault).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByLabelText("Arrival")).not.toBeOnTheScreen());
    expect(screen.queryByText("Remove from Continue watching")).not.toBeOnTheScreen();
    expect(deletes()).toEqual([`/api/continue-watching/movies/${arrival.id}/v1`]);
  });

  test("the ⋯ button on a hovered episode tile opens the same menu, which removes the episode's show", async () => {
    await renderWithPendingRemovals();
    jest.useFakeTimers();
    await fireEvent(screen.getByLabelText("Half Loop"), "pointerEnter");
    await act(() => jest.advanceTimersByTime(400));
    jest.useRealTimers();

    await fireEvent.press(screen.getByLabelText("More actions"));
    await fireEvent.press(screen.getByText("Remove from Continue watching"));

    await waitFor(() => expect(screen.queryByLabelText("Half Loop")).not.toBeOnTheScreen());
    expect(screen.getByLabelText("Arrival")).toBeOnTheScreen();
    expect(deletes()).toEqual([`/api/continue-watching/shows/${severance.id}/v1`]);
  });

  test("tiles in other rows have no menu", async () => {
    await renderWithPendingRemovals();

    await fireEvent(screen.getByLabelText("Dune"), "contextMenu", { preventDefault: () => {} });

    expect(screen.queryByText("Remove from Continue watching")).not.toBeOnTheScreen();
  });

  describe("on TV", () => {
    const view = (View as unknown as { prototype: { requestTVFocus?: () => void } }).prototype;
    let focused: string[];

    beforeEach(() => {
      jest.spyOn(Platform, "isTV", "get").mockReturnValue(true);
      focused = [];
      view.requestTVFocus = function (this: { props: { accessibilityLabel?: string } }) {
        focused.push(this.props.accessibilityLabel ?? "");
      };
    });

    afterEach(() => {
      delete view.requestTVFocus;
    });

    test("a long press opens the menu", async () => {
      await renderWithPendingRemovals();

      await fireEvent(screen.getByLabelText("Arrival"), "longPress");
      await fireEvent.press(screen.getByText("Remove from Continue watching"));

      await waitFor(() => expect(screen.queryByLabelText("Arrival")).not.toBeOnTheScreen());
      expect(deletes()).toEqual([`/api/continue-watching/movies/${arrival.id}/v1`]);
    });

    test("removing a tile focuses the tile that takes its place", async () => {
      await renderWithPendingRemovals();

      await fireEvent(screen.getByLabelText("Arrival"), "longPress");
      await fireEvent.press(screen.getByText("Remove from Continue watching"));

      await waitFor(() => expect(focused).toEqual(["Half Loop"]));
    });

    test("removing the last tile focuses the tile before it", async () => {
      await renderWithPendingRemovals();

      await fireEvent(screen.getByLabelText("Half Loop"), "longPress");
      await fireEvent.press(screen.getByText("Remove from Continue watching"));

      await waitFor(() => expect(focused).toEqual(["Arrival"]));
    });
  });
});
