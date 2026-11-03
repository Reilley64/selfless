import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { GridSelectors } from "./GridSelectors";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

test("choosing a genre or sort on the web updates the URL query", async () => {
  jest.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
    input === "/api/genres/v1?kind=movie"
      ? Response.json({
          genres: [
            { name: "Comedy", count: 12 },
            { name: "Drama", count: 40 },
          ],
        })
      : new Response(null, { status: 404 }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });

  await renderRouter(
    {
      "movies/index": () => (
        <QueryClientProvider client={client}>
          <GridSelectors kind="movie" />
        </QueryClientProvider>
      ),
    },
    { initialUrl: "/movies" },
  );

  await fireEvent.press(screen.getByLabelText("Genre"));
  await fireEvent.press(await screen.findByText("Comedy"));
  expect(screen).toHavePathname("/movies");
  expect(screen).toHaveSearchParams({ genre: "Comedy" });

  await fireEvent.press(screen.getByLabelText("Sort"));
  await fireEvent.press(screen.getByText("Recently added"));
  expect(screen).toHaveSearchParams({ genre: "Comedy", sort: "added" });
});
