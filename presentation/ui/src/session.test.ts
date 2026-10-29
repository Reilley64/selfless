import { getMe } from "@selfless/api-client";
import { type SessionStorage, startTvSession, startWebSession } from "./session";

const token = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M";
const user = { id: "01890a5d-ac96-774b-bcce-b302099a8057", name: "Noor", colour: "amber", symbol: "ring" };
const theo = {
  token: "Zt0bX9qL2wR5yV8nH3kM6pJ1sF4dG7aC0eU2iO5lN8Q",
  user: { id: "01890a5d-ac96-774b-bcce-b302099a8058", name: "Theo", colour: "blue", symbol: "triangle" },
};

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

const serverUrl = "http://192.168.1.20:8080";

function mockServer({ signedIn = true, issued = [{ token, user }, theo], ended = [] as string[] } = {}) {
  const caller = (init?: RequestInit) => {
    const bearer = new Headers(init?.headers).get("Authorization");
    return issued.find(({ token }) => bearer === `Bearer ${token}` && !ended.includes(token));
  };
  return jest.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const path = String(input).replace(serverUrl, "");
    switch (`${init?.method ?? "GET"} ${path}`) {
      case "GET /actuator/info/v1":
        return Response.json({ build: { name: "selfless", version: "0.1.0" } });
      case "POST /api/sessions/v1": {
        const signedIn = issued.find(({ user }) => user.name === JSON.parse(String(init?.body)).name);
        return Response.json(signedIn, { status: 201 });
      }
      case "GET /api/me/v1":
        return signedIn && caller(init) ? Response.json(caller(init)?.user) : Response.json(unauthenticated, { status: 401 });
      case "DELETE /api/sessions/current/v1":
        return caller(init) ? new Response(null, { status: 204 }) : Response.json(unauthenticated, { status: 401 });
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

test("the TV checks the server address with /actuator/info/v1, remembers it and calls the API there", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();

  const connected = await startTvSession(storage, "appleTv").connect(" 192.168.1.20:8080/ ");
  const restarted = startTvSession(storage, "appleTv");
  await getMe();

  expect(connected).toBe(true);
  expect(restarted.state().serverUrl).toBe(serverUrl);
  expect(fetch.mock.calls.map(([input]) => input)).toEqual([`${serverUrl}/actuator/info/v1`, `${serverUrl}/api/me/v1`]);
});

test("the TV doesn't remember an address where no selfless server answers", async () => {
  jest.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    if (String(input).startsWith("http://192.168.1.21:8080/")) {
      return new Response("Not found", { status: 404 });
    }
    throw new TypeError("Network request failed");
  });
  const storage = fakeStorage();
  const session = startTvSession(storage, "appleTv");

  const unreachable = await session.connect("192.168.1.99:8080");
  const notSelfless = await session.connect("192.168.1.21:8080");

  expect([unreachable, notSelfless]).toEqual([false, false]);
  expect(startTvSession(storage, "appleTv").state().serverUrl).toBeUndefined();
});

async function connectedTv(storage = fakeStorage(), clientKind: "appleTv" | "googleTv" = "appleTv") {
  const session = startTvSession(storage, clientKind);
  await session.connect(serverUrl);
  return session;
}

test("signing in on a TV sends its client kind and device name, and adds the user to the TV's picker list", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage, "googleTv");

  await session.signIn("Noor", "correct horse", "Living Room");

  const signInCall = fetch.mock.calls.find(([input]) => input === `${serverUrl}/api/sessions/v1`);
  expect(JSON.parse(String(signInCall?.[1]?.body))).toEqual({
    name: "Noor",
    password: "correct horse",
    clientKind: "googleTv",
    deviceName: "Living Room",
  });
  expect(startTvSession(storage, "googleTv").state().users).toEqual([{ token, ...user }]);
});

test("each user who signs in on the TV joins its picker list", async () => {
  mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);

  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");

  expect(startTvSession(storage, "appleTv").state().users).toEqual([
    { token, ...user },
    { token: theo.token, ...theo.user },
  ]);
});

test("on a cold start nobody is picked, and picking a user needs no password and makes API calls as them", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const signedIn = await connectedTv(storage);
  await signedIn.signIn("Noor", "correct horse", "Living Room");
  await signedIn.signIn("Theo", "battery staple", "Living Room");

  const session = startTvSession(storage, "appleTv");
  const pickedAtStart = session.state().currentUserId;
  session.pick(theo.user.id);
  await getMe();

  expect(pickedAtStart).toBeUndefined();
  expect(session.state().currentUserId).toBe(theo.user.id);
  expect(sentHeaders(fetch, `${serverUrl}/api/me/v1`).get("Authorization")).toBe(`Bearer ${theo.token}`);
});

test("a user who signs in on the TV opens the app as them", async () => {
  mockServer();
  const session = await connectedTv();

  await session.signIn("Theo", "battery staple", "Living Room");

  expect(session.state().currentUserId).toBe(theo.user.id);
  expect(session.token()).toBe(theo.token);
});

test("switching user returns to the picker without ending any session", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");

  session.switchUser();

  expect(session.state().currentUserId).toBeUndefined();
  expect(session.state().users).toEqual([{ token, ...user }]);
  expect(fetch.mock.calls.some(([, init]) => init?.method === "DELETE")).toBe(false);
});

test("signing in again as a user on the picker replaces their entry and ends their old session", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  const newToken = "Hn4cT7vB1xK9sW3eR6yU0iP2oL5aZ8dF1gJ4mQ7wE0r";
  mockServer({ issued: [{ token: newToken, user }, theo] });

  await session.signIn("Noor", "correct horse", "Living Room");

  expect(startTvSession(storage, "appleTv").state().users).toEqual([
    { token: newToken, ...user },
    { token: theo.token, ...theo.user },
  ]);
  expect(sentHeaders(fetch, `${serverUrl}/api/sessions/current/v1`).get("Authorization")).toBe(`Bearer ${token}`);
});

test("removing a user ends their session and takes them off the picker", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  session.switchUser();

  await session.remove(user.id);

  expect(sentHeaders(fetch, `${serverUrl}/api/sessions/current/v1`).get("Authorization")).toBe(`Bearer ${token}`);
  expect(startTvSession(storage, "appleTv").state()).toEqual({
    serverUrl,
    users: [{ token: theo.token, ...theo.user }],
    currentUserId: undefined,
  });
});

test("removing the last user forgets the server address", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  session.switchUser();

  await session.remove(user.id);
  await session.remove(theo.user.id);

  expect(sentHeaders(fetch, `${serverUrl}/api/sessions/current/v1`).get("Authorization")).toBe(`Bearer ${theo.token}`);
  expect(session.state()).toEqual({ serverUrl: undefined, users: [], currentUserId: undefined });
  expect(startTvSession(storage, "appleTv").state()).toEqual(session.state());
});

test("a 401 during use returns to the picker with that user dimmed", async () => {
  mockServer({ ended: [theo.token] });
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");

  const response = await getMe();

  expect(response.status).toBe(401);
  expect(session.state().currentUserId).toBeUndefined();
  expect(startTvSession(storage, "appleTv").state().users).toEqual([
    { token, ...user },
    { token: theo.token, ...theo.user, signedOut: true },
  ]);
});

test("refreshing the picker updates each user from the server and dims those whose sign-in has ended", async () => {
  const fetch = mockServer();
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  session.switchUser();
  const renamed = { ...user, name: "Noor A." };
  mockServer({ issued: [{ token, user: renamed }, theo], ended: [theo.token] });

  await session.refresh();

  const meCalls = fetch.mock.calls.filter(([input]) => input === `${serverUrl}/api/me/v1`);
  expect(meCalls.map(([, init]) => new Headers(init?.headers).get("Authorization"))).toEqual([
    `Bearer ${token}`,
    `Bearer ${theo.token}`,
  ]);
  expect(startTvSession(storage, "appleTv").state().users).toEqual([
    { token, ...renamed },
    { token: theo.token, ...theo.user, signedOut: true },
  ]);
});

test("a dimmed user can be removed too", async () => {
  mockServer({ ended: [token] });
  const storage = fakeStorage();
  const session = await connectedTv(storage);
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  session.switchUser();
  await session.refresh();

  await session.remove(user.id);

  expect(startTvSession(storage, "appleTv").state().users).toEqual([{ token: theo.token, ...theo.user }]);
});
