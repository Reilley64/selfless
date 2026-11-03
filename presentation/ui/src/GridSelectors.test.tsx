import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, renderRouter, screen } from "expo-router/testing-library";
import { BackHandler, Platform } from "react-native";
import { GridSelectors } from "./GridSelectors";

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
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

afterEach(() => {
  jest.restoreAllMocks();
});

async function renderSelectors() {
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
}

test("choosing a genre or sort on the web updates the URL query", async () => {
  await renderSelectors();

  await fireEvent.press(screen.getByLabelText("Genre"));
  await fireEvent.press(await screen.findByText("Comedy"));
  expect(screen).toHavePathname("/movies");
  expect(screen).toHaveSearchParams({ genre: "Comedy" });

  await fireEvent.press(screen.getByLabelText("Sort"));
  await fireEvent.press(screen.getByText("Recently added"));
  expect(screen).toHaveSearchParams({ genre: "Comedy", sort: "added" });
});

test("on TV, Back closes an open selector", async () => {
  jest.spyOn(Platform, "isTV", "get").mockReturnValue(true);
  const backHandlers: Parameters<typeof BackHandler.addEventListener>[1][] = [];
  jest.spyOn(BackHandler, "addEventListener").mockImplementation((_, handler) => {
    backHandlers.push(handler);
    return { remove: () => backHandlers.splice(backHandlers.indexOf(handler), 1) };
  });
  await renderSelectors();
  await fireEvent.press(screen.getByLabelText("Genre"));
  await screen.findByText("Comedy");

  await act(() => {
    expect(backHandlers.at(-1)?.({ type: "hardwareBackPress", timeStamp: 0 })).toBe(true);
  });

  expect(screen.queryByText("Comedy")).not.toBeOnTheScreen();
  expect(screen).toHaveSearchParams({});
});
