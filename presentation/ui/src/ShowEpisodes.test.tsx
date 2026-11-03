import type { EpisodeList, ShowPage } from "@selfless/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { Platform } from "react-native";
import { ShowEpisodes } from "./ShowEpisodes";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

const showId = "0190f3c4-6c7d-7e8f-9a0b-1c2d3e4f5a6b";
const seasonOne = "0190f3c4-7d8e-7f90-a1b2-c3d4e5f6a7b8";
const seasonTwo = "0190f3c4-8e9f-7a01-b2c3-d4e5f6a7b8c9";
const specials = "0190f3c4-9a0b-7c1d-8e2f-3a4b5c6d7e8f";
const helloMsCobel = "0190f3c4-ab1c-7d2e-8f3a-4b5c6d7e8f9a";

const severance: ShowPage = {
  id: showId,
  title: "Severance",
  year: 2022,
  certification: "TV-MA",
  status: "Continuing",
  network: "Apple TV+",
  seasonCount: 2,
  genres: ["Drama", "Mystery"],
  rating: 8.7,
  plot: "Office workers have their memories split.",
  cast: [],
  images: { fanart: null, logo: null },
  seasons: [
    { id: seasonOne, number: 1, name: "Season 1", episodeCount: 1 },
    { id: seasonTwo, number: 2, name: "Season 2", episodeCount: 1 },
    { id: specials, number: 0, name: "Specials", episodeCount: 1 },
  ],
  play: {
    verb: "resume",
    resumePosition: 1534.2,
    season: { id: seasonTwo, number: 2 },
    episode: { id: helloMsCobel, number: 1, title: "Hello, Ms. Cobel" },
  },
  moreLikeThis: [],
};

const episode = (
  id: string,
  number: number,
  title: string,
  runtimeMinutes: number | null,
  { progress = null, watched = false }: { progress?: number | null; watched?: boolean } = {},
) => ({
  id,
  number,
  title,
  runtimeMinutes,
  plot: `The plot of ${title}.`,
  thumb: null,
  progress,
  watched,
});

const episodes: Record<string, EpisodeList> = {
  [seasonOne]: {
    episodes: [episode("0190f3c4-bc2d-7e3f-8a4b-5c6d7e8f9a0b", 1, "Good News About Hell", 57, { watched: true })],
  },
  [seasonTwo]: { episodes: [episode(helloMsCobel, 1, "Hello, Ms. Cobel", 50, { progress: 0.51 })] },
  [specials]: { episodes: [episode("0190f3c4-cd3e-7f4a-8b5c-6d7e8f9a0b1c", 1, "Main Title Sequence", null)] },
};

async function renderShowEpisodes(initialUrl: string) {
  jest.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const seasonId = String(input).match(/^\/api\/seasons\/([^/]+)\/episodes\/v1$/)?.[1];
    return seasonId && episodes[seasonId] ? Response.json(episodes[seasonId]) : new Response(null, { status: 404 });
  });
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });
  await renderRouter(
    {
      "shows/[showId]": () => (
        <QueryClientProvider client={client}>
          <ShowEpisodes show={severance} />
        </QueryClientProvider>
      ),
      "play/episodes/[episodeId]": () => null,
      "episodes/[episodeId]": () => null,
    },
    { initialUrl },
  );
}

test("the show page opens on the season of the episode that Play would play", async () => {
  await renderShowEpisodes(`/shows/${showId}`);

  expect(await screen.findByText("Hello, Ms. Cobel")).toBeOnTheScreen();
  expect(screen.getByLabelText("Season")).toHaveTextContent(/Season 2/);
  expect(screen.getByText("50m")).toBeOnTheScreen();
  expect(screen.getByText("The plot of Hello, Ms. Cobel.")).toBeOnTheScreen();
});

test("each episode shows the user's progress bar or watched mark", async () => {
  await renderShowEpisodes(`/shows/${showId}`);

  expect(await screen.findByRole("progressbar")).toHaveAccessibilityValue({ now: 51 });
  expect(screen.queryByLabelText("Watched")).toBeNull();

  await fireEvent.press(screen.getByLabelText("Season"));
  await fireEvent.press(screen.getByText("Season 1"));

  expect(await screen.findByLabelText("Watched")).toBeOnTheScreen();
  expect(screen.queryByRole("progressbar")).toBeNull();
});

test("a season in the URL opens that season", async () => {
  await renderShowEpisodes(`/shows/${showId}?season=0`);

  expect(await screen.findByText("Main Title Sequence")).toBeOnTheScreen();
  expect(screen.getByLabelText("Season")).toHaveTextContent(/Specials/);
});

test("changing season updates the query string and the episode list", async () => {
  await renderShowEpisodes(`/shows/${showId}`);
  await screen.findByText("Hello, Ms. Cobel");

  await fireEvent.press(screen.getByLabelText("Season"));
  await fireEvent.press(screen.getByText("Season 1"));

  expect(screen).toHavePathname(`/shows/${showId}`);
  expect(screen).toHaveSearchParams({ showId, season: "1" });
  expect(await screen.findByText("Good News About Hell")).toBeOnTheScreen();
  expect(screen.getByText("57m")).toBeOnTheScreen();
});

test("choosing an episode opens the player", async () => {
  await renderShowEpisodes(`/shows/${showId}`);

  await fireEvent.press(await screen.findByLabelText("Play E1 Hello, Ms. Cobel"));

  expect(screen).toHavePathname(`/play/episodes/${helloMsCobel}`);
});

test("an episode's ⋯ button opens the episode page", async () => {
  await renderShowEpisodes(`/shows/${showId}`);

  await fireEvent.press(await screen.findByLabelText("More about Hello, Ms. Cobel"));

  expect(screen).toHavePathname(`/episodes/${helloMsCobel}`);
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

  afterEach(() => {
    jest.restoreAllMocks();
  });

  test("the season column lists the seasons with Specials last and opens on the Play target's season", async () => {
    await renderShowEpisodes(`/shows/${showId}`);

    expect(await screen.findByText("Hello, Ms. Cobel")).toBeOnTheScreen();
    expect(screen.getAllByRole("tab").map((x) => x.props.accessibilityLabel)).toEqual(["Season 1", "Season 2", "Specials"]);
    expect(screen.getByRole("tab", { selected: true })).toHaveAccessibleName("Season 2");
  });

  test("moving focus to a season shows its episodes", async () => {
    await renderShowEpisodes(`/shows/${showId}`);
    await screen.findByText("Hello, Ms. Cobel");

    await fireEvent(screen.getByRole("tab", { name: "Season 1" }), "focus");

    expect(await screen.findByText("Good News About Hell")).toBeOnTheScreen();
    expect(screen.getByRole("tab", { selected: true })).toHaveAccessibleName("Season 1");
  });

  test("a long press on an episode opens the episode page", async () => {
    await renderShowEpisodes(`/shows/${showId}`);

    await fireEvent(await screen.findByLabelText("Play E1 Hello, Ms. Cobel"), "longPress");

    expect(screen).toHavePathname(`/episodes/${helloMsCobel}`);
  });
});
