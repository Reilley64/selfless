import { getMe, provideSessionToken } from "@selfless/api-client";

const user = { id: "01890a5d-ac96-774b-bcce-b302099a8057", name: "Noor", colour: "amber", symbol: "ring" };

function mockFetch() {
  return jest.spyOn(globalThis, "fetch").mockImplementation(async () => Response.json(user));
}

function sentHeaders(fetch: ReturnType<typeof mockFetch>) {
  return new Headers(fetch.mock.calls[0][1]?.headers);
}

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

afterEach(() => {
  jest.restoreAllMocks();
  provideSessionToken(() => undefined);
});

test("API calls send the session token from the function the app provides", async () => {
  const fetch = mockFetch();
  provideSessionToken(() => "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M");

  await getMe();

  expect(sentHeaders(fetch).get("Authorization")).toBe("Bearer q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M");
});

test("API calls send no Authorization header without a session token", async () => {
  const fetch = mockFetch();

  await getMe();

  expect(sentHeaders(fetch).has("Authorization")).toBe(false);
});
