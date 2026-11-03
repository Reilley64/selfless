import type { Tile, TilePage } from "@selfless/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { Platform } from "react-native";
import { MoviesGrid } from "./GridPage";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

beforeEach(() => {
  jest.spyOn(Platform, "isTV", "get").mockReturnValue(true);
});

afterEach(() => {
  jest.restoreAllMocks();
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

const movie = (index: number): Tile => ({
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
});

const pages: Record<string, TilePage> = {
  "/api/movies/v1": { tiles: [movie(1), movie(2)], nextCursor: null },
  "/api/movies/v1?sort=added": { tiles: [movie(9)], nextCursor: null },
};

async function renderMovies() {
  jest.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    if (input === "/api/genres/v1?kind=movie") {
      return Response.json({ genres: [{ name: "Comedy", count: 12 }] });
    }
    const page = pages[String(input)];
    return page ? Response.json(page) : new Response(null, { status: 404 });
  });
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });
  await renderRouter(
    {
      "movies/index": () => (
        <QueryClientProvider client={client}>
          <MoviesGrid />
        </QueryClientProvider>
      ),
    },
    { initialUrl: "/movies" },
  );
  await screen.findByLabelText("Movie 1");
}

test("on TV, choosing a sort above the grid reloads it from the first page", async () => {
  await renderMovies();

  await fireEvent.press(screen.getByLabelText("Sort"));
  await fireEvent.press(screen.getByText("Recently added"));

  expect(await screen.findByLabelText("Movie 9")).toBeOnTheScreen();
  expect(screen.queryByLabelText("Movie 1")).not.toBeOnTheScreen();
});
