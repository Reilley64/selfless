import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react-native";
import { ServerInfo } from "./ServerInfo";

test("shows the server's name and version from the info endpoint", async () => {
  jest.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
    input === "/actuator/info/v1"
      ? Response.json({
          build: { name: "selfless", version: "1.2.3" },
          git: { branch: "main", commit: { id: "56ad9034", time: "2026-10-29T06:12:00Z" } },
        })
      : new Response(null, { status: 404 }),
  );

  await render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })}>
      <ServerInfo />
    </QueryClientProvider>,
  );

  expect(await screen.findByText("selfless")).toBeOnTheScreen();
  expect(screen.getByText("1.2.3")).toBeOnTheScreen();
});
