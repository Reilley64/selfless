import type { Home as HomeData, Tile } from "@selfless/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { Home } from "./Home";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

const tile = (id: string, kind: "movie" | "show", title: string): Tile => ({
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

async function renderHome() {
  jest
    .spyOn(globalThis, "fetch")
    .mockImplementation(async (input) =>
      input === "/api/home/v1" ? Response.json(home) : new Response(null, { status: 404 }),
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
  await screen.findByLabelText("Severance");
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

test("a movie tile opens the movie page", async () => {
  await renderHome();

  await fireEvent.press(screen.getAllByLabelText("Arrival")[0]);

  expect(screen).toHavePathname(`/movies/${arrival.id}`);
});
