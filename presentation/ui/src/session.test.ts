import { getMe } from "@selfless/api-client";
import { type SessionStorage, startWebSession } from "./session";

const token = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M";
const user = { id: "01890a5d-ac96-774b-bcce-b302099a8057", name: "Noor", colour: "amber", symbol: "ring" };

function fakeStorage(): SessionStorage {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

const unauthenticated = {
  type: "urn:selfless:problem:unauthenticated",
  title: "The request needs a valid session token.",
  status: 401,
};

function mockServer({ signedIn = true } = {}) {
  return jest.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    switch (`${init?.method} ${input}`) {
      case "POST /api/sessions/v1":
        return Response.json({ token, user }, { status: 201 });
      case "GET /api/me/v1":
        return signedIn ? Response.json(user) : Response.json(unauthenticated, { status: 401 });
      case "DELETE /api/sessions/current/v1":
        return new Response(null, { status: 204 });
      default:
        return new Response(null, { status: 404 });
    }
  });
}

function sentHeaders(fetch: ReturnType<typeof mockServer>, path: string) {
  const call = fetch.mock.calls.findLast(([input]) => input === path);
  return new Headers(call?.[1]?.headers);
}

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

afterEach(() => {
  jest.restoreAllMocks();
});

test("signing in keeps the token in storage, and API calls send it after a reload", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  await startWebSession(storage).signIn("Noor", "correct horse", "Firefox on macOS");

  const reloaded = startWebSession(storage);
  await getMe();

  expect(reloaded.token()).toBe(token);
  expect(sentHeaders(fetch, "/api/me/v1").get("Authorization")).toBe(`Bearer ${token}`);
});

test("a 401 clears the token and tells the app before the call returns", async () => {
  mockServer({ signedIn: false });
  const storage = fakeStorage();
  const session = startWebSession(storage);
  await session.signIn("Noor", "correct horse", "Firefox on macOS");
  const tokenWhenNotified = jest.fn(() => session.token());
  session.subscribe(tokenWhenNotified);

  const response = await getMe();

  expect(response.status).toBe(401);
  expect(tokenWhenNotified).toHaveReturnedWith(undefined);
  expect(startWebSession(storage).token()).toBeUndefined();
});

test("signing out ends the session on the server and clears the token", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = startWebSession(storage);
  await session.signIn("Noor", "correct horse", "Firefox on macOS");

  await session.signOut();

  expect(sentHeaders(fetch, "/api/sessions/current/v1").get("Authorization")).toBe(`Bearer ${token}`);
  expect(session.token()).toBeUndefined();
  expect(startWebSession(storage).token()).toBeUndefined();
});

test("API calls send no Authorization header before anyone signs in", async () => {
  const fetch = mockServer();
  startWebSession(fakeStorage());

  await getMe();

  expect(sentHeaders(fetch, "/api/me/v1").has("Authorization")).toBe(false);
});
